import { describe, expect, it, vi } from 'vitest';
import type { McpRequest } from '../mcp/types';
import { dispatchMcpRequest, type McpBridgeDeps } from './mcpBridge';

const req = (method: string, params: Record<string, unknown> = {}): McpRequest => ({
  id: 'test-1',
  windowLabel: 'main',
  method,
  params,
});

function deps() {
  const platform = {
    windowLabel: 'main',
    windowLabels: vi.fn(async () => ['main', 'space-1']),
    readTextFile: vi.fn(async () => ({ text: 'from disk', mtimeMs: 1 })),
    listDir: vi.fn(async () => [
      { name: '10-old.md', path: '/root/10-old.md', isDir: false },
      { name: '2-new.md', path: '/root/2-new.md', isDir: false },
    ]),
    stat: vi.fn(async (path: string) => ({ mtimeMs: 1, size: 1, isDir: path === '/root/topics' })),
  };
  const docs = {
    store: { docs: [{ id: 'd1', path: '/root/open.md' }], activeId: 'd1' },
    textOf: vi.fn(() => '# Live buffer\n\nUnsaved text.'),
    setContentByPath: vi.fn(async () => ({ id: 'd1', changed: true })),
  };
  const explorer = {
    state: { root: '/root' },
    everyFile: vi.fn(async () => ['/root/open.md', '/root/other.md']),
    create: vi.fn(async () => '/root/Created.md'),
    rename: vi.fn(async (_from: string, to: string) => `/root/${to}`),
    move: vi.fn(async (_from: string, to: string, name: string) => `${to}/${name}`),
    trash: vi.fn(async () => true),
  };
  return {
    platform,
    docs,
    explorer,
    asDeps: { platform, docs, explorer } as unknown as McpBridgeDeps,
  };
}

describe('MCP bridge', () => {
  it('reads the live buffer, not the file on disk', async () => {
    const d = deps();
    const result = (await dispatchMcpRequest(req('calmmd_read_document', { path: 'open.md' }), d.asDeps)) as {
      text: string;
      headings: { text: string }[];
    };
    expect(result.text).toContain('Unsaved text.');
    expect(result.headings[0].text).toBe('Live buffer');
    expect(d.platform.readTextFile).not.toHaveBeenCalled();
  });

  it('rejects paths outside the open folder', async () => {
    const d = deps();
    await expect(dispatchMcpRequest(req('calmmd_read_document', { path: '../secret.md' }), d.asDeps)).rejects.toThrow(
      '..',
    );
  });

  it('trashes with the MCP confirmation instead of a webview modal', async () => {
    const d = deps();
    expect(await dispatchMcpRequest(req('calmmd_trash_document', { path: 'open.md', confirm: true }), d.asDeps)).toEqual({
      path: '/root/open.md',
      trashed: true,
    });
    expect(d.explorer.trash).toHaveBeenCalledWith('/root/open.md', false, true);
  });

  it('routes writes through the document manager', async () => {
    const d = deps();
    const result = await dispatchMcpRequest(
      req('calmmd_write_document', { path: 'open.md', content: '\n\n## Next', mode: 'append' }),
      d.asDeps,
    );
    expect(result).toEqual({ path: '/root/open.md', changed: true });
    expect(d.docs.setContentByPath).toHaveBeenCalledWith('/root/open.md', '\n\n## Next', {
      mode: 'append',
      flush: true,
    });
  });

  it('filters document listings by folder', async () => {
    const d = deps();
    d.explorer.everyFile.mockResolvedValue(['/root/a.md', '/root/notes/b.md', '/root/else/c.md']);
    const result = (await dispatchMcpRequest(req('calmmd_list_documents', { folder: 'notes' }), d.asDeps)) as {
      path: string;
    }[];
    expect(result.map((x) => x.path)).toEqual(['/root/notes/b.md']);
  });

  it('triages a folder from live note text', async () => {
    const d = deps();
    d.platform.listDir.mockResolvedValue([
      { name: 'topics', path: '/root/topics', isDir: true },
      { name: 'open.md', path: '/root/open.md', isDir: false },
    ]);
    const result = (await dispatchMcpRequest(req('calmmd_triage_folder'), d.asDeps)) as {
      subfolders: string[];
      notes: { path: string; title: string; excerpt: string }[];
    };
    expect(result.subfolders).toEqual(['topics']);
    expect(result.notes[0].title).toBe('Live buffer');
    expect(result.notes[0].excerpt).toContain('Unsaved text.');
  });

  it('previews an organization plan without moving until asked', async () => {
    const d = deps();
    const plan = { moves: [{ from: 'open.md', toFolder: 'topics', newName: 'better-name.md', reason: 'Ambiguous name' }] };
    const result = (await dispatchMcpRequest(req('calmmd_apply_organization', { plan }), d.asDeps)) as {
      applied: boolean;
      plan: { from: string; to: string }[];
    };
    expect(result.applied).toBe(false);
    expect(result.plan[0].to).toBe('/root/topics/better-name.md');
    expect(d.explorer.move).not.toHaveBeenCalled();
  });

  it('applies an organization plan through the explorer', async () => {
    const d = deps();
    const plan = { moves: [{ from: 'open.md', toFolder: 'topics', newName: 'better-name.md' }] };
    await dispatchMcpRequest(req('calmmd_apply_organization', { plan, apply: true }), d.asDeps);
    expect(d.explorer.move).toHaveBeenCalledWith('/root/open.md', '/root/topics', 'better-name.md');
  });

  it('moves a single document with an optional rename', async () => {
    const d = deps();
    const result = await dispatchMcpRequest(
      req('calmmd_move_document', { path: 'open.md', folder: 'topics', name: 'renamed.md' }),
      d.asDeps,
    );
    expect(result).toEqual({ from: '/root/open.md', to: '/root/topics/renamed.md' });
  });
});
