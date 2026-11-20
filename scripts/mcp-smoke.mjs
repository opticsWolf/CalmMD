const URL = process.env.CALMMD_MCP_URL;
const TOKEN = process.env.CALMMD_MCP_TOKEN;
if (!URL || !TOKEN) throw new Error('Set CALMMD_MCP_URL and CALMMD_MCP_TOKEN');
const CLIENT = { name: 'calmmd-full-test', version: '1.0.0' };

let session;
let nextId = 1;
const results = [];

function parseBody(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith('{')) return JSON.parse(trimmed);
  const data = trimmed.split(/\r?\n/).filter((l) => l.startsWith('data:')).map((l) => l.slice(5).trim()).filter(Boolean);
  return data.length ? JSON.parse(data[data.length - 1]) : null;
}

async function post(body) {
  const headers = {
    'content-type': 'application/json',
    accept: 'application/json, text/event-stream',
    authorization: `Bearer ${TOKEN}`,
    'Mcp-Method': body.method,
  };
  const name = body.params?.name ?? body.params?.uri;
  if (typeof name === 'string') headers['Mcp-Name'] = name;
  if (session) {
    headers['mcp-session-id'] = session;
    headers['mcp-protocol-version'] = '2026-07-28';
    body.params = {
      ...(body.params ?? {}),
      _meta: {
        'io.modelcontextprotocol/protocolVersion': '2026-07-28',
        'io.modelcontextprotocol/clientCapabilities': {},
        'io.modelcontextprotocol/clientInfo': CLIENT,
      },
    };
  }
  const res = await fetch(URL, { method: 'POST', headers, body: JSON.stringify(body) });
  const parsed = parseBody(await res.text());
  if (!res.ok || parsed?.error) throw new Error(JSON.stringify(parsed?.error ?? { status: res.status }));
  return { parsed, res };
}

async function init() {
  const { res } = await post({
    jsonrpc: '2.0', id: nextId++, method: 'initialize',
    params: { protocolVersion: '2026-07-28', capabilities: {}, clientInfo: CLIENT },
  });
  session = res.headers.get('mcp-session-id');
  await post({ jsonrpc: '2.0', method: 'notifications/initialized', params: {} });
}

async function tool(name, args = {}) {
  const { parsed } = await post({
    jsonrpc: '2.0', id: nextId++, method: 'tools/call',
    params: { name, arguments: args },
  });
  const raw = parsed.result?.content?.[0]?.text ?? '';
  let value = raw;
  try { value = JSON.parse(raw); } catch {}
  if (parsed.result?.isError) throw new Error(typeof value === 'string' ? value : JSON.stringify(value));
  return value;
}

async function prompt(name, args = {}) {
  const { parsed } = await post({
    jsonrpc: '2.0', id: nextId++, method: 'prompts/get',
    params: { name, arguments: args },
  });
  return parsed.result;
}

async function resource(uri) {
  const { parsed } = await post({
    jsonrpc: '2.0', id: nextId++, method: 'resources/read',
    params: { uri },
  });
  return parsed.result?.contents?.[0]?.text ?? '';
}

async function check(label, fn) {
  try {
    const value = await fn();
    results.push([label, 'PASS', value]);
    console.log(`PASS ${label}`);
    return value;
  } catch (e) {
    results.push([label, 'FAIL', e.message]);
    console.log(`FAIL ${label}: ${e.message}`);
    return null;
  }
}

const topics = [
  ['Garden', 'compost', 'seedlings'],
  ['Budget', 'invoices', 'quarterly'],
  ['Travel', 'trains', 'packing'],
  ['Reading', 'books', 'highlights'],
  ['Health', 'sleep', 'walking'],
];
const docNames = Array.from({ length: 20 }, (_, i) => `note-${String(i + 1).padStart(2, '0')}.md`);

await init();

await check('initialize + notifications/initialized', async () => 'session ' + session);

await check('tools/list', async () => {
  const { parsed } = await post({ jsonrpc: '2.0', id: nextId++, method: 'tools/list', params: {} });
  return parsed.result.tools.length + ' tools';
});

await check('prompts/list', async () => {
  const { parsed } = await post({ jsonrpc: '2.0', id: nextId++, method: 'prompts/list', params: {} });
  return parsed.result.prompts.map((p) => p.name).join(', ');
});

await check('resources/list + templates/list', async () => {
  const list = await post({ jsonrpc: '2.0', id: nextId++, method: 'resources/list', params: {} });
  const templates = await post({ jsonrpc: '2.0', id: nextId++, method: 'resources/templates/list', params: {} });
  return `${list.parsed.result.resources.length} resources, ${templates.parsed.result.resourceTemplates.length} templates`;
});

await check('calmmd_status', async () => {
  const s = await tool('calmmd_status');
  if (!s.root) throw new Error('no folder open');
  return s.root;
});
await check('calmmd_windows', async () => (await tool('calmmd_windows')).windows.length + ' windows');
await check('calmmd_create_folder Areas', async () => (await tool('calmmd_create_folder', { name: 'Areas' })).path);
await check('calmmd_create_folder exports', async () => (await tool('calmmd_create_folder', { name: 'exports' })).path);

for (let i = 0; i < docNames.length; i++) {
  const [topic, a, b] = topics[i % topics.length];
  const name = docNames[i];
  await check(`create + write ${name}`, async () => {
    try { await tool('calmmd_create_document', { name, text: '' }); } catch (e) {
      if (!/exist|already/i.test(e.message)) throw e;
    }
    const content = `---\ntitle: ${topic} note ${i + 1}\ncategory: ${topic}\n---\n\n# ${topic} note ${i + 1}\n\nThis note collects thoughts about ${a}, ${b} and small practical follow-ups.\n\n- Review ${a}\n- Try ${b}\n- Share with the team\n${i === 0 ? '\nSee also [garden note](note-05.md).\n' : ''}`;
    await tool('calmmd_write_document', { path: name, content, mode: 'replace', flush: true });
    return topic;
  });
}

await check('calmmd_list_documents', async () => (await tool('calmmd_list_documents')).length + ' documents');
await check('calmmd_read_document', async () => (await tool('calmmd_read_document', { path: 'note-01.md' })).text.length + ' chars');
await check('calmmd_search', async () => JSON.stringify(await tool('calmmd_search', { query: 'garden' })).length + ' chars');
await check('calmmd_outline', async () => (await tool('calmmd_outline', { path: 'note-02.md' })).headings.length + ' headings');

await check('prompt calmmd_summarize', async () => {
  const p = await prompt('calmmd_summarize', { path: 'note-01.md' });
  return p.messages[0].content.text.slice(0, 90).replace(/\s+/g, ' ');
});
await check('prompt calmmd_follow_up_guide', async () => {
  const p = await prompt('calmmd_follow_up_guide', { path: 'note-02.md' });
  return p.messages[0].content.text.slice(0, 90).replace(/\s+/g, ' ');
});
await check('prompt calmmd_organize_folder', async () => {
  const p = await prompt('calmmd_organize_folder', { limit: 20 });
  return p.messages[0].content.text.slice(0, 90).replace(/\s+/g, ' ');
});

await check('calmmd_triage_folder', async () => {
  const t = await tool('calmmd_triage_folder', { limit: 20, excerptChars: 120 });
  return `${t.total} notes, ${t.subfolders.length} subfolders`;
});

await check('calmmd_rename_document', async () => (await tool('calmmd_rename_document', { path: 'note-19.md', name: 'renamed-note-19.md' })).path);
await check('calmmd_move_document', async () => (await tool('calmmd_move_document', { path: 'renamed-note-19.md', folder: 'Areas', name: 'moved-note-19.md' })).to);

const plan = { moves: [
  { from: 'note-05.md', toFolder: 'Areas', newName: 'garden-plan.md', reason: 'Garden content' },
  { from: 'note-06.md', toFolder: 'Areas', reason: 'Budget content' },
] };
await check('calmmd_apply_organization preview', async () => {
  const r = await tool('calmmd_apply_organization', { plan });
  if (r.applied !== false) throw new Error('preview applied changes');
  return r.plan.length + ' planned moves';
});
await check('calmmd_apply_organization apply', async () => {
  const r = await tool('calmmd_apply_organization', { plan, apply: true });
  return `${r.plan.length} applied, ${r.failed.length} failed`;
});
await check('link rewrite after move', async () => {
  const text = (await tool('calmmd_read_document', { path: 'note-01.md' })).text;
  if (!text.includes('Areas/garden-plan.md')) throw new Error('link was not retargeted');
  return 'link retargeted';
});

await check('calmmd_export_html', async () => {
  const r = await tool('calmmd_export_html', { path: 'note-01.md', out: 'exports/note-01.html', prefs: { toc: true } });
  return r.path;
});
await check('calmmd_export_pdf', async () => {
  const r = await tool('calmmd_export_pdf', {
    path: 'note-02.md', out: 'exports/note-02.pdf',
    prefs: { cover: true, pdf: { template: 'minimal' } },
    cover: { title: 'MCP smoke cover', author: 'CalmMD test' },
  });
  return r.path;
});

await check('calmmd_trash_document', async () => tool('calmmd_trash_document', { path: 'note-20.md', confirm: true }));

await check('resource calmmd://status', async () => (await resource('calmmd://status')).slice(0, 60).replace(/\s+/g, ' '));
await check('resource calmmd://windows', async () => (await resource('calmmd://windows')).slice(0, 60).replace(/\s+/g, ' '));
await check('resource calmmd://document', async () => (await resource('calmmd://document/note-01.md')).slice(0, 60).replace(/\s+/g, ' '));
await check('resource calmmd://folder', async () => (await resource('calmmd://folder/Areas')).slice(0, 60).replace(/\s+/g, ' '));

await check('final calmmd_status', async () => (await tool('calmmd_status')).openDocuments.length + ' open documents');

console.log('\n===== SUMMARY =====');
for (const [label, state, value] of results) console.log(`${state}\t${label}\t${String(value).slice(0, 120).replace(/\s+/g, ' ')}`);
const passed = results.filter((r) => r[1] === 'PASS').length;
console.log(passed + '/' + results.length + ' checks passed');
if (passed !== results.length) process.exitCode = 1;
