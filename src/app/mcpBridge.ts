import { frontMatterFields } from '../core/cover';
import { basename, isMarkdownPath, isWithin, joinPath, samePath } from '../core/paths';
import { extractHeadings } from '../core/outline';
import type { McpRequest } from '../mcp/types';
import type { Platform } from '../platform/types';
import type { Documents } from './documents';
import type { Explorer } from './explorer';

export interface McpBridgeDeps {
  platform: Platform;
  docs: Documents;
  explorer: Explorer;
}

type Params = Record<string, unknown>;
const str = (v: unknown, name: string): string => {
  if (typeof v !== 'string' || !v.trim()) throw new Error(`Missing "${name}"`);
  return v.trim();
};
const bool = (v: unknown, fallback = false): boolean => (typeof v === 'boolean' ? v : fallback);

/** Resolves a path against the current Space root and rejects escapes. */
function rootOf(deps: McpBridgeDeps): string {
  const root = deps.explorer.state.root;
  if (!root) throw new Error('Open a folder in CalmMD first');
  return root;
}

function resolvePath(deps: McpBridgeDeps, p: Params, key = 'path'): string {
  const root = rootOf(deps);
  const raw = str(p[key], key);
  const absolute = raw.startsWith('/') || /^[a-zA-Z]:[\\/]/.test(raw) || raw.startsWith('\\\\');
  const path = absolute ? raw : joinPath(root, raw);
  const normalized = path.replace(/\\/g, '/');
  if (normalized.split('/').includes('..')) throw new Error('Paths may not contain ".."');
  if (!isWithin(path, root)) throw new Error('That path is outside the open folder');
  return path;
}

async function readLiveText(deps: McpBridgeDeps, path: string): Promise<string> {
  const open = deps.docs.store.docs.find((d) => d.path && samePath(d.path, path));
  if (open) return deps.docs.textOf(open.id);
  return (await deps.platform.readTextFile(path)).text;
}

function intInRange(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? Math.max(min, Math.min(max, Math.floor(value))) : fallback;
}

async function ensureFolder(deps: McpBridgeDeps, path: string): Promise<void> {
  const root = rootOf(deps);
  if (!isWithin(path, root)) throw new Error('That path is outside the open folder');
  const relative = path.slice(root.length).replace(/^[\\/]+/, '');
  if (!relative) return;
  let parent = root;
  for (const segment of relative.split(/[\\/]+/).filter(Boolean)) {
    const candidate = joinPath(parent, segment);
    const info = await deps.platform.stat(candidate).catch(() => null);
    if (!info) await deps.explorer.create(parent, segment, 'folder');
    else if (!info.isDir) throw new Error(`Not a folder: ${candidate}`);
    parent = candidate;
  }
}

interface OrganizationMove {
  from: string;
  toFolder: string;
  name: string;
  to: string;
  reason: string;
}

async function organizationMoves(deps: McpBridgeDeps, plan: unknown, folder: string): Promise<OrganizationMove[]> {
  if (!plan || typeof plan !== 'object') throw new Error('Missing organization plan');
  const raw = (plan as { moves?: unknown }).moves;
  if (!Array.isArray(raw) || raw.length === 0) throw new Error('Organization plan has no moves');
  const root = rootOf(deps);
  const moves = await Promise.all(raw.map(async (value, index) => {
    if (!value || typeof value !== 'object') throw new Error(`Invalid move #${index + 1}`);
    const item = value as Record<string, unknown>;
    const from = resolvePath(deps, { path: str(item.from, `moves[${index}].from`) });
    if (!isWithin(from, folder)) throw new Error('A move source is outside the folder being organised');
    const toFolder = resolvePath(deps, { path: str(item.toFolder, `moves[${index}].toFolder`) });
    if (!isWithin(toFolder, root)) throw new Error('A move destination is outside the open folder');
    const name = typeof item.newName === 'string' && item.newName.trim() ? item.newName.trim() : basename(from);
    if (name.includes('/') || name.includes('\\') || name === '.' || name === '..') throw new Error(`Invalid file name: ${name}`);
    return {
      from,
      toFolder,
      name,
      to: joinPath(toFolder, name),
      reason: typeof item.reason === 'string' ? item.reason : '',
    };
  }));
  const key = (path: string) => path.replace(/\\/g, '/').toLowerCase();
  const targets = new Set<string>();
  for (const move of moves) {
    if (targets.has(key(move.to))) throw new Error(`Duplicate destination: ${move.to}`);
    targets.add(key(move.to));
  }
  return moves;
}

/** One MCP method, executed against the live app state. Exported for tests. */
export async function dispatchMcpRequest(req: McpRequest, deps: McpBridgeDeps): Promise<unknown> {
  const p = req.params ?? {};
  switch (req.method) {
    case 'calmmd_status': {
      const active = deps.docs.store.docs.find((d) => d.id === deps.docs.store.activeId);
      return {
        windowLabel: deps.platform.windowLabel,
        windows: await deps.platform.windowLabels().catch(() => []),
        root: deps.explorer.state.root,
        openDocuments: deps.docs.store.docs.filter((d) => d.path).map((d) => d.path),
        activeDocument: active?.path ?? null,
      };
    }

    case 'calmmd_list_documents': {
      const files = await deps.explorer.everyFile();
      const markdownOnly = bool(p.markdownOnly, true);
      const folder = p.folder ? resolvePath(deps, { path: str(p.folder, 'folder') }, 'path') : null;
      return files
        .filter((path) => !folder || isWithin(path, folder))
        .filter((path) => !markdownOnly || isMarkdownPath(path))
        .map((path) => ({
          path,
          name: basename(path),
          open: deps.docs.store.docs.some((d) => d.path && samePath(d.path, path)),
        }));
    }

    case 'calmmd_read_document': {
      const path = resolvePath(deps, p);
      const text = await readLiveText(deps, path);
      return { path, text, headings: extractHeadings(text) };
    }

    case 'calmmd_search': {
      const root = rootOf(deps);
      const query = str(p.query, 'query');
      return deps.platform.searchFiles(root, query, {
        caseSensitive: bool(p.caseSensitive),
        wholeWord: bool(p.wholeWord),
      });
    }

    case 'calmmd_outline': {
      const path = resolvePath(deps, p);
      return { path, headings: extractHeadings(await readLiveText(deps, path)) };
    }

    case 'calmmd_create_document': {
      const root = rootOf(deps);
      const folder = p.folder ? resolvePath(deps, { path: str(p.folder, 'folder') }, 'path') : root;
      const name = str(p.name, 'name');
      const path = await deps.explorer.create(folder, name, 'file');
      if (!path) throw new Error('Could not create the document');
      if (typeof p.text === 'string' && p.text.length) {
        await deps.docs.setContentByPath(path, p.text, { mode: 'replace', flush: true });
      }
      return { path };
    }

    case 'calmmd_create_folder': {
      const parent = p.folder ? resolvePath(deps, { path: str(p.folder, 'folder') }, 'path') : rootOf(deps);
      const path = await deps.explorer.create(parent, str(p.name, 'name'), 'folder');
      if (!path) throw new Error('Could not create the folder');
      return { path };
    }

    case 'calmmd_rename_document': {
      const path = resolvePath(deps, p);
      const name = str(p.name, 'name');
      const to = await deps.explorer.rename(path, name);
      if (!to) throw new Error('Could not rename the document');
      return { path: to };
    }

    case 'calmmd_move_document': {
      const path = resolvePath(deps, p);
      const folder = resolvePath(deps, { path: str(p.folder, 'folder') }, 'path');
      await ensureFolder(deps, folder);
      const name = typeof p.name === 'string' && p.name.trim() ? p.name.trim() : basename(path);
      const to = await deps.explorer.move(path, folder, name);
      if (!to) throw new Error('Could not move the document');
      return { from: path, to };
    }

    case 'calmmd_write_document': {
      const path = resolvePath(deps, p);
      const content = typeof p.content === 'string' ? p.content : str(p.text, 'content');
      const mode = p.mode === 'append' || p.mode === 'prepend' ? p.mode : 'replace';
      const result = await deps.docs.setContentByPath(path, content, { mode, flush: bool(p.flush, true) });
      return { path, changed: result.changed };
    }

    case 'calmmd_trash_document': {
      const path = resolvePath(deps, p);
      if (!bool(p.confirm)) throw new Error('Move to trash requires confirm: true');
      // The MCP `confirm` argument is the caller's confirmation; the webview cannot show a modal.
      const ok = await deps.explorer.trash(path, false, true);
      return { path, trashed: ok };
    }

    case 'calmmd_triage_folder': {
      const folder = p.folder ? resolvePath(deps, { path: str(p.folder, 'folder') }, 'path') : rootOf(deps);
      const limit = intInRange(p.limit, 200, 1, 500);
      const excerptChars = intInRange(p.excerptChars, 600, 80, 4000);
      const entries = await deps.platform.listDir(folder);
      const files = entries.filter((e) => !e.isDir && isMarkdownPath(e.name));
      const chosen = files.slice(0, limit);
      const notes = await Promise.all(chosen.map(async (entry) => {
        const text = await readLiveText(deps, entry.path);
        const headings = extractHeadings(text);
        const stat = await deps.platform.stat(entry.path).catch(() => null);
        return {
          path: entry.path,
          name: entry.name,
          title: headings[0]?.text || frontMatterFields(text).title || basename(entry.path),
          headings: headings.slice(0, 8).map((h) => h.text),
          frontMatter: frontMatterFields(text),
          excerpt: text.slice(0, excerptChars),
          mtimeMs: stat?.mtimeMs ?? null,
          open: deps.docs.store.docs.some((d) => d.path && samePath(d.path, entry.path)),
        };
      }));
      return {
        folder,
        subfolders: entries.filter((e) => e.isDir).map((e) => e.name),
        notes,
        total: files.length,
        truncated: files.length > chosen.length,
        limit,
        excerptChars,
      };
    }

    case 'calmmd_apply_organization': {
      const folder = p.folder ? resolvePath(deps, { path: str(p.folder, 'folder') }, 'path') : rootOf(deps);
      const moves = await organizationMoves(deps, p.plan, folder);
      const key = (path: string) => path.replace(/\\/g, '/').toLowerCase();
      const existing = new Set((await deps.explorer.everyFile()).map(key));
      const sources = new Set(moves.map((m) => key(m.from)));
      for (const move of moves) {
        if (existing.has(key(move.to)) && !sources.has(key(move.to))) throw new Error(`Destination already exists: ${move.to}`);
      }
      const preview = moves.map(({ from, toFolder, name, to, reason }) => ({ from, toFolder, newName: name, to, reason }));
      const warning = 'Folders are created and notes are moved through CalmMD; links are rewritten after each move. Check the status bar for failures.';
      if (!bool(p.apply)) return { folder, applied: false, plan: preview, warning };

      for (const toFolder of new Set(moves.map((m) => m.toFolder))) await ensureFolder(deps, toFolder);
      const applied: typeof preview = [];
      const failed: { from: string; to: string; error: string }[] = [];
      for (const move of moves) {
        try {
          const to = await deps.explorer.move(move.from, move.toFolder, move.name);
          if (!to) throw new Error('CalmMD rejected the move');
          applied.push({ from: move.from, toFolder: move.toFolder, newName: move.name, to, reason: move.reason });
        } catch (e) {
          failed.push({ from: move.from, to: move.to, error: e instanceof Error ? e.message : String(e) });
        }
      }
      return { folder, applied: true, plan: applied, failed, warning };
    }

    default:
      throw new Error(`Unknown MCP method: ${req.method}`);
  }
}

/** Installs the webview half of the bridge. */
export function createMcpBridge(deps: McpBridgeDeps) {
  deps.platform.onMcpRequest((req) => {
    void dispatchMcpRequest(req, deps).then(
      (result) => void deps.platform.mcpRespond({ id: req.id, ok: true, result }),
      (error: unknown) =>
        void deps.platform.mcpRespond({
          id: req.id,
          ok: false,
          error: error instanceof Error ? error.message : String(error),
        }),
    );
  });
}
