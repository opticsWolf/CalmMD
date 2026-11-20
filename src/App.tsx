import { Compartment, EditorState, type Extension, type StateEffect } from '@codemirror/state';
import { EditorView, type ViewUpdate } from '@codemirror/view';
import { createEffect, createMemo, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { createStore } from 'solid-js/store';
import { buildCommands } from './app/appCommands';
import { CommandRegistry } from './app/commands';
import { createDocuments, docName } from './app/documents';
import { editorMenuItems, spellingItems, wordAt } from './app/editorMenu';
import { recheckSpelling, spelling } from './editor/spelling';
import { createExplorer } from './app/explorer';
import { createMcpBridge } from './app/mcpBridge';
import type { McpConnectionInfo } from './mcp/types';
import type { WorkspaceEvent, WorkspaceSnapshot } from './workspace/types';
import type { SpellDictionary } from './core/spellDictionaries';
import { openLink } from './app/links';
import { createPreview } from './app/preview';
import { parseSession, type Session, type SidebarView } from './app/session';
import { createSpaces, type SpaceRecord } from './app/spaces';
import { createExporter, type ExportPrefs } from './app/exporter';
import { BUILTIN_THEMES, isBuiltinTheme, loadUserThemes, type UserTheme } from './app/themes';
import type { BaseTheme } from './export/build';
import { ExportDialog } from './ui/ExportDialog';
import { createSettings, SPELL_LANGUAGES, TEXT_WIDTH_SCALE, VIEW_MODES, type PreviewTheme, type ViewMode } from './app/settings';
import { appMenus } from './app/appMenus';
import type { ColorTheme } from './app/colorThemes';
import { NATIVE_EDIT_ITEMS, resolveMenu, type ResolvedEntry, type TopMenu } from './app/menuModel';
import type { NativeMenu, NativeMenuItem } from './platform/types';
import { AboutDialog, ShortcutsDialog } from './ui/HelpDialogs';
import { coverFromDoc } from './core/cover';
import { inlineImages, localImages, planZipBundle } from './core/bundle';
import { createZip } from './core/zip';
import { LogoMark } from './ui/icons';
import { colorValue } from './editor/colors';
import { createAssets } from './app/assets';
import { findImageLinks, isImagePath } from './core/assets';
import { MenuBar } from './ui/MenuBar';
import { notePicker } from './editor/notePicker';
import { linksTo, parseWiki, resolveWiki, rewriteLinks, type Backlink } from './core/noteLinks';
import { markdownUrl } from './core/assets';
import { SearchPanel } from './ui/SearchPanel';
import { QuickView } from './ui/QuickView';
import { DrawingEditor } from './ui/DrawingEditor';
import type { TableActions } from './editor/live/tableWidget';
import { headingLine } from './core/headings';
import type { FileTab } from './ui/TabStrip';
import { htmlToMarkdown, worthConverting } from './editor/richPaste';
import { inCode } from './app/editorMenu';
import { compareVersions, isCheckDue, parseRelease, RELEASES_API, shouldAnnounce, type Release } from './app/updates';
import { drawingFileName, drawingMarkdown, drawingTitle } from './core/drawings';
import { withAssetVersions } from './platform/assetVersions';
import { imagesChanged } from './editor/live/widgets';
import { insertBlock } from './editor/format';
import { findImageLinks as findDrawingLinks } from './core/assets';
import { resolveLocalPath } from './app/links';
import { HistoryPanel, versionTime } from './ui/HistoryPanel';
import { VersionView } from './ui/VersionView';
import { createHistory, type Version } from './app/history';
import { minimalChange } from './core/text';
import { fileKind } from './core/fileKinds';
import welcomeText from './assets/welcome.md?raw';
import guideText from './assets/markdown-guide.md?raw';
import { isEmbeddableUrl } from './render/embeds';
import { BRAND } from './brand';
import { formatBinding } from './core/keys';
import { extractHeadings, headingIndexAt, type Heading } from './core/outline';
import { basename, dirname, isMarkdownPath, isPlainTextPath, isWithin, joinPath, pathKey, samePath, stem } from './core/paths';
import { notifySavedFiles } from './platform/savedFiles';
import { countStats, type TextStats } from './core/stats';
import { editorExtensions, isUserEdit, markdownSupport } from './editor/extensions';
import { scrollLineToTop } from './editor/geometry';
import { livePreview, refreshLive } from './editor/live/decorations';
import { slashCommands, type SlashItem } from './editor/slash';
import type { Platform } from './platform';
import { createRenderer, resolveImageSrc, slugify } from './render/markdown';
import { AppearanceMenu } from './ui/AppearanceMenu';
import { CommandPalette, type PaletteItem, type PalettePrefix, type PaletteSource } from './ui/CommandPalette';
import { createConfirm } from './ui/ConfirmDialog';
import { ConflictBanner } from './ui/ConflictBanner';
import { createContextMenu, type MenuItem } from './ui/ContextMenu';
import { Sidebar } from './ui/Sidebar';
import { IconSidebar } from './ui/sidebarIcons';
import { StatusBar } from './ui/StatusBar';
import { TabStrip } from './ui/TabStrip';
import { CustomizeToolbar, SelectionBubble, Toolbar } from './ui/Toolbar';
import { ViewSwitch } from './ui/ViewSwitch';

const EMPTY_STATS: TextStats = { words: 0, chars: 0, lines: 1 };
const showsEditor = (m: ViewMode) => m !== 'preview';
const showsPreview = (m: ViewMode) => m === 'split' || m === 'preview';

export default function App(props: { platform: Platform }) {
  // Files saved through a save dialog (Save As, exports) show in the Files sidebar right away.
  // Files saved through a save dialog (Save As, exports) show in the Files sidebar right away, and
  // file URLs change when a drawing is saved so pictures of it reload.
  const assetVersions = withAssetVersions(notifySavedFiles(props.platform, (path) => void explorer.refreshDir(dirname(path))));
  const platform = assetVersions.platform;
  const isMac = platform.isMac;
  const { settings, update, load: loadSettings, flush: flushSettings } = createSettings(platform);
  const [message, setMessage] = createSignal<{ text: string; action?: { label: string; run: () => void } } | null>(null);
  const [stats, setStats] = createSignal<TextStats>(EMPTY_STATS);
  const [mcpInfo, setMcpInfo] = createSignal<McpConnectionInfo | null>(null);
  const [selection, setSelection] = createSignal<TextStats | null>(null);
  const { confirm, prompt, ConfirmDialog } = createConfirm();

  // ---------- Window and Space identity ----------
  const label = platform.windowLabel;
  const isMain = label === 'main';
  const spaces = createSpaces(platform, () => platform.broadcast('spaces'));
  /** The Space this window holds, or null for a loose window. */
  const [space, setSpace] = createSignal<{ id: string; name: string } | null>(null);
  const [recentSpaces, setRecentSpaces] = createSignal<SpaceRecord[]>([]);
  const refreshSpaces = async () => setRecentSpaces(await spaces.list());
  const draftOwner = () => (space() ? `space:${space()!.id}` : `window:${label}`);

  // The colour scheme actually in effect ("system" resolved), for diagrams.
  const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');
  const [systemDark, setSystemDark] = createSignal(darkQuery.matches);
  darkQuery.addEventListener('change', (e) => setSystemDark(e.matches));
  const effectiveScheme = (): 'light' | 'dark' =>
    settings.colorScheme === 'system' ? (systemDark() ? 'dark' : 'light') : settings.colorScheme;
  /** The colour theme in effect: the preferred light or dark one. */
  const activeColorTheme = () => {
    return effectiveScheme() === 'dark' ? settings.darkTheme : settings.lightTheme;
  };
  /** Picking a theme makes it the preferred one of its kind, and shows it now. */
  const pickColorTheme = (t: ColorTheme) => {
    const patch: Partial<typeof settings> = t.kind === 'light' ? { lightTheme: t.id } : { darkTheme: t.id };
    if (effectiveScheme() !== t.kind) patch.colorScheme = t.kind;
    update(patch);
  };

  // View mode is per window (and saved with its Space). The setting is the default for new windows.
  const [chosenMode, setModeSignal] = createSignal<ViewMode>(settings.viewMode);
  // Plain text files (.txt) always show as text in the editor, whatever the window's mode.
  const plainText = () => isPlainTextPath(activeDoc()?.path ?? '');
  const mode = (): ViewMode => (plainText() ? 'source' : chosenMode());

  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  /** A status bar message. `sticky` ones stay until the next message (a new-version notice). */
  const notify = (text: string, action?: { label: string; run: () => void }, sticky = false) => {
    setMessage({ text, action });
    clearTimeout(messageTimer);
    // Messages with an action stay a little longer.
    if (!sticky) messageTimer = setTimeout(() => setMessage(null), action ? 9000 : 5000);
  };

  let view: EditorView | undefined;
  let preview: ReturnType<typeof createPreview> | undefined;
  let statsTimer: ReturnType<typeof setTimeout> | undefined;

  // ---------- Commands (registered before the editor so its keymap can include them) ----------
  const registry = new CommandRegistry();
  const hint = (id: string) => {
    const k = registry.get(id)?.keys;
    return k ? formatBinding(k, isMac) : undefined;
  };
  const [palette, setPalette] = createSignal<string | null>(null);
  const [customizing, setCustomizing] = createSignal(false);

  // ---------- Slash commands ----------
  const slashItems = (): SlashItem[] => {
    const item = (label: string, id: string, keywords?: string, detail?: string): SlashItem => ({
      label,
      keywords,
      detail,
      run: () => registry.run(id),
    });
    return [
      item('Heading 1', 'format.heading1', 'h1 title'),
      item('Heading 2', 'format.heading2', 'h2'),
      item('Heading 3', 'format.heading3', 'h3'),
      item('Bulleted list', 'format.bulletList', 'ul bullet'),
      item('Numbered list', 'format.orderedList', 'ol ordered'),
      item('Task list', 'format.taskList', 'todo checkbox'),
      item('Quote', 'format.quote', 'blockquote'),
      item('Code block', 'insert.codeBlock', 'fence pre'),
      item('Table', 'insert.table', 'grid'),
      item('Divider', 'insert.hr', 'hr rule line'),
      item('Image', 'insert.image', 'picture'),
      item('Math block', 'insert.math', 'latex katex equation'),
      item('Mermaid diagram', 'insert.mermaid', 'chart flowchart graph'),
      item('Drawing', 'insert.drawing', 'excalidraw sketch whiteboard draw picture'),
      item('Footnote', 'insert.footnote', 'note reference'),
    ];
  };

  // Markdown or plain text, per editor state (set when a document becomes active).
  const languageCompartment = new Compartment();
  const languageFor = () => (plainText() ? [] : markdownSupport);
  const extensions = editorExtensions({
    onUpdate,
    language: languageCompartment.of(markdownSupport),
    extra: [
      registry.editorKeymap(isMac),
      slashCommands(slashItems, () => settings.slashCommands, [
        notePicker({
          files: () => noteFiles(),
          allFiles: () => explorer.everyFile().catch(() => []),
          docPath: () => activeDoc()?.path ?? null,
          root: () => explorer.state.root,
          // An open note's current text, otherwise the file on disk.
          readText: async (p) => {
            const open = store.docs.find((d) => d.path && samePath(d.path, p));
            return open ? docs.textOf(open.id) : (await platform.readTextFile(p)).text;
          },
        }),
      ]),
      // Pasted and dropped images are saved as files and linked (app/assets.ts).
      // Double-click a drawing's picture to edit it.
      EditorView.domEventHandlers({
        dblclick: (e, v) => {
          const img = (e.target as HTMLElement).closest('.cm-live-image');
          if (!img) return false;
          const path = drawingAt(v.posAtDOM(img));
          if (!path) return false;
          e.preventDefault();
          openDrawing(path);
          return true;
        },
      }),
      EditorView.domEventHandlers({
        paste: (e, v) => {
          const data = e.clipboardData;
          if (!data) return false;
          // Formatted text from a web page, Word or Google Docs becomes Markdown (not inside code).
          // Ctrl/Cmd+Shift+V pastes only the plain text, so it skips this.
          const html = data.getData('text/html');
          if (html && worthConverting(html) && !inCode(v.state, v.state.selection.main.head)) {
            const md = htmlToMarkdown(html);
            if (md) {
              e.preventDefault();
              v.dispatch(v.state.replaceSelection(md), { userEvent: 'input.paste', scrollIntoView: true });
              return true;
            }
          }
          // Office apps put a picture of the text on the clipboard too: text wins when there is any.
          if (data.getData('text/plain').trim()) return false;
          const files = [...data.files].filter((f) => f.type.startsWith('image/'));
          if (!files.length) return false;
          e.preventDefault();
          void insertImageFiles(files);
          return true;
        },
        drop: (e, v) => {
          const files = [...(e.dataTransfer?.files ?? [])].filter((f) => f.type.startsWith('image/'));
          if (!files.length) return false;
          e.preventDefault();
          const pos = v.posAtCoords({ x: e.clientX, y: e.clientY });
          if (pos !== null) v.dispatch({ selection: { anchor: pos } });
          void insertImageFiles(files);
          return true;
        },
      }),
    ],
  });

  // ---------- Links between notes ----------
  /** Notes the [[ picker and wiki links can reach: the open folder's files plus open tabs. */
  async function noteFiles(): Promise<string[]> {
    const fromFolder = await explorer.allFiles().catch(() => [] as string[]);
    const open = store.docs.map((d) => d.path).filter((p): p is string => !!p);
    const seen = new Set<string>();
    return [...open, ...fromFolder].filter((p) => {
      const k = p.toLowerCase();
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
  }
  /** Follows a [[wiki link]]: the note, then its heading if one is named. */
  async function openWiki(target: string) {
    const w = parseWiki(target);
    if (!w) return;
    const from = activeDoc()?.path ?? null;
    const path = w.name ? resolveWiki(w.name, from, await noteFiles()) : from;
    if (!path) return notify(explorer.state.root ? `No note named “${w.name}” in this folder` : 'Open a folder to follow wiki links');
    if (!from || !samePath(path, from)) await docs.openPaths([path]);
    if (w.heading)
      setTimeout(() => {
        const h = headings().find((x) => x.text.toLowerCase() === w.heading!.toLowerCase() || slugify(x.text) === slugify(w.heading!));
        if (h) gotoLine(h.line);
      }, 60);
  }

  // "Linked from": scanned when the active note or the folder changes, or the window regains focus.
  const [backlinks, setBacklinks] = createSignal<Backlink[] | null>(null);
  let backlinkSeq = 0;
  async function refreshBacklinks() {
    const path = activeDoc()?.path ?? null;
    const root = explorer.state.root;
    const mine = ++backlinkSeq;
    if (!path || !root || !isWithin(path, root)) return setBacklinks(null);
    const name = stem(path);
    // The folder search finds candidate files fast; links are then checked properly.
    const queries = [...new Set([name, markdownUrl(name)])];
    const hits = (await Promise.all(queries.map((q) => platform.searchFiles(root, q, { caseSensitive: false, wholeWord: false }).catch(() => [])))).flat();
    const candidates = [...new Set(hits.map((h) => h.path))].filter((p) => !samePath(p, path));
    const files = await noteFiles();
    const out: Backlink[] = [];
    for (const c of candidates.slice(0, 200)) {
      const text = await platform.readTextFile(c).then((r) => r.text, () => '');
      out.push(...linksTo(text, c, path, files));
    }
    if (mine === backlinkSeq) setBacklinks(out);
  }
  createEffect(
    on(
      () => [activeDoc()?.path, explorer.state.root, sidebar.open && sidebar.view === 'outline'] as const,
      ([, , visible]) => {
        if (visible) void refreshBacklinks();
      },
    ),
  );

  // ---------- Quick view ----------
  const [quickView, setQuickView] = createSignal<string | null>(null);
  /** Notes open in the editor, drawings in the drawing editor; anything else opens in Quick view. */
  function openFileOrView(path: string) {
    if (fileKind(path) === 'note') {
      setActiveFile(null);
      void docs.openPaths([path]);
    } else openFileTab(path, true);
  }

  // ---------- File tabs ----------
  // Drawings, images, PDFs and other files open in tabs next to the notes and fill the main area.
  // (Space in the sidebar still gives a quick peek.)
  const [fileTabs, setFileTabs] = createSignal<FileTab[]>([]);
  const [activeFile, setActiveFile] = createSignal<string | null>(null);
  let fileTabCount = 0;
  const activeFileTab = () => fileTabs().find((t) => t.id === activeFile()) ?? null;
  function openFileTab(path: string, exists: boolean) {
    const open = fileTabs().find((t) => samePath(t.path, path));
    if (open) return setActiveFile(open.id);
    const tab: FileTab = { id: `file-${++fileTabCount}`, path, exists };
    setFileTabs([...fileTabs(), tab]);
    setActiveFile(tab.id);
  }
  function closeFileTab(id: string) {
    const list = fileTabs();
    const i = list.findIndex((t) => t.id === id);
    if (i === -1) return;
    const next = list.filter((t) => t.id !== id);
    setFileTabs(next);
    if (activeFile() === id) {
      setActiveFile(next[Math.min(i, next.length - 1)]?.id ?? null);
      if (!activeFile()) queueMicrotask(() => view?.focus());
    }
  }
  // Opening or switching notes shows the note.
  createEffect(on(() => store.activeId, () => setActiveFile(null), { defer: true }));

  // ---------- Drawings (Excalidraw) ----------
  // A drawing is its own file next to the notes (Name.excalidraw.svg), opened in a tab. Notes show it
  // as a picture. A note waiting for a new drawing gets its link on the drawing's first save.
  const waitingForDrawing = new Map<string, string>(); // drawing path key -> document id
  const showDrawing = (path: string, exists: boolean) => openFileTab(path, exists);
  const openDrawing = (path: string) => showDrawing(path, true);

  /** Insert → Drawing (insert: true) or File → New Drawing: asks for a name, then opens the editor. */
  async function newDrawing(insert: boolean) {
    const d = activeDoc();
    const dir = d?.path ? dirname(d.path) : explorer.state.root;
    if (!dir) {
      notify(insert ? 'Save this note first, so the drawing has a folder to go in' : 'Open a folder or save a note first, so the drawing has a place to go');
      return;
    }
    const name = await prompt({
      title: 'New Drawing',
      message: `It’s saved as its own file in “${basename(dir) || dir}”${insert ? ', and shown in this note as a picture' : ''}.`,
      placeholder: 'For example: Kitchen layout',
      primary: 'Create',
    });
    const file = name ? drawingFileName(name) : null;
    if (!file) return;
    const path = joinPath(dir, file);
    const exists = !!(await platform.stat(path).catch(() => null));
    if (insert && exists) insertDrawingLink(path);
    else if (insert && d) waitingForDrawing.set(pathKey(path), d.id);
    showDrawing(path, exists);
  }
  function insertDrawingLink(path: string) {
    if (!view) return;
    const md = drawingMarkdown(activeDoc()?.path ?? null, path);
    view.dispatch({ ...insertBlock(view.state, md, md.length), userEvent: 'input' });
  }
  function drawingSaved(path: string, first: boolean) {
    assetVersions.changed(path);
    imagesChanged();
    void explorer.refreshDir(dirname(path));
    const waiting = waitingForDrawing.get(pathKey(path));
    if (first && waiting) {
      waitingForDrawing.delete(pathKey(path));
      if (waiting === store.activeId) insertDrawingLink(path);
      else notify(`Saved ${drawingTitle(path)}. Its note isn’t open here, so add the picture with Insert → Image.`);
    }
    view?.dispatch({ effects: refreshLive.of(null) });
    if (preview && showsPreview(mode())) {
      preview.invalidate();
      preview.renderNow();
    }
  }
  /** The drawing file an image link at pos points to, if it's a drawing. */
  function drawingAt(pos: number): string | null {
    if (!view) return null;
    // Link offsets cover only the destination, so take the image link on the same line closest to pos.
    const line = view.state.doc.lineAt(pos);
    const link = findDrawingLinks(view.state.doc.toString())
      .filter((l) => l.from >= line.from && l.to <= line.to)
      .sort((x, y) => Math.abs(x.from - pos) - Math.abs(y.from - pos))[0];
    const local = link ? resolveLocalPath(link.url, activeDoc()?.path ?? null) : null;
    return local && fileKind(local) === 'drawing' ? local : null;
  }
  createEffect(() => explorer.setShowAllFiles(settings.showAllFiles));

  // ---------- Images ----------
  /** Inserts image links at the cursor, each on its own line when the line already has text. */
  function insertImageLinks(links: (string | null)[]) {
    const ok = links.filter((l): l is string => !!l);
    if (!view || !ok.length) return;
    const line = view.state.doc.lineAt(view.state.selection.main.from);
    const before = line.text.slice(0, view.state.selection.main.from - line.from).trim() ? '\n' : '';
    view.dispatch(view.state.replaceSelection(before + ok.map((l) => `![](${l})`).join('\n')), { userEvent: 'input.paste', scrollIntoView: true });
    view.focus();
    void explorer.refresh(); // the assets folder may be new
  }
  async function insertImageFiles(files: File[]) {
    const docPath = activeDoc()?.path ?? null;
    const links: (string | null)[] = [];
    for (const f of files) links.push(await assets.saveData(docPath, new Uint8Array(await f.arrayBuffer()), f.type, f.name));
    insertImageLinks(links);
  }
  /** Picks an image file and puts its link in place of [from, to) (Replace Image…, Locate…). */
  async function replaceImageAt(from: number, to: number) {
    const [picked] = await platform.openImageDialog();
    if (!picked || !view) return;
    const link = await assets.importFile(activeDoc()?.path ?? null, picked);
    if (!link) return;
    const len = view.state.doc.length;
    if (to > len) return; // the document changed meanwhile
    view.dispatch({ changes: { from, to, insert: link }, userEvent: 'input.replace' });
    view.focus();
    void explorer.refresh();
  }
  /** A missing image's "Locate…": fixes the first image link with that source. */
  async function locateImage(src: string) {
    if (!view) return;
    const link = findImageLinks(view.state.doc.toString()).find((l) => l.url === src);
    if (link) await replaceImageAt(link.from, link.to);
  }
  async function insertImagePaths(paths: string[]) {
    const docPath = activeDoc()?.path ?? null;
    const links: (string | null)[] = [];
    for (const p of paths) links.push(await assets.importFile(docPath, p));
    insertImageLinks(links);
  }

  // ---------- Live mode ----------
  // A compartment per editor state. New states start in the current mode, and existing
  // states are reconfigured when they become active.
  const liveCompartment = new Compartment();
  const tableRenderer = createRenderer();
  const renderEnv = () => ({ docPath: activeDoc()?.path ?? null, resolveAsset: platform.assetUrl });
  const liveExtension = livePreview({
    resolveImage: (src) => resolveImageSrc(src, renderEnv()),
    renderMarkdown: (md) => tableRenderer.render(md, renderEnv()),
    openLink: (href) => followLink(href),
    locateImage: (src) => void locateImage(src),
    colorScheme: () => effectiveScheme(),
  });
  const liveFor = (m: ViewMode) => (m === 'live' ? liveExtension : []);
  // Spellcheck. Where the OS spellchecker answers us (Windows, macOS), CalmMD draws the underlines
  // itself (editor/spelling.ts) and the webview's own checker is off. Elsewhere the webview's checker
  // is used, configured with contentDOM attributes; `lang` picks the dictionary.
  const spellCompartment = new Compartment();
  const spellCache = new Map<string, Extension>();
  const [osSpelling, setOsSpelling] = createSignal(false);
  const [dictionaries, setDictionaries] = createSignal<SpellDictionary[]>([]);
  const registeredDictionaryCommands = new Set<string>();
  const registerDictionaryCommands = (list: SpellDictionary[]) => {
    const commands = list.flatMap((dictionary) => {
      const tag = dictionary.languages[0];
      const result = [{
        id: `spell.${tag}`,
        title: `Spelling Language: ${dictionary.label}`,
        category: 'Spelling',
        when: () => !!dictionaries().find((d) => d.id === dictionary.id)?.installed,
        run: () => {
          update({ spellLang: tag, spellcheck: true });
          notify(`Spellcheck: ${dictionary.label}`);
        },
      }];
      if (!dictionary.installed) result.push({
        id: `spell.install.${dictionary.id}`,
        title: `Download ${dictionary.label} Dictionary`,
        category: 'Spelling',
        when: () => !dictionaries().find((d) => d.id === dictionary.id)?.installed,
        run: async () => {
          notify(`Downloading ${dictionary.label} dictionary…`);
          try {
            await platform.spellInstallDictionary(dictionary.id);
            await refreshDictionaries();
            spellCache.clear();
            update({ spellLang: tag, spellcheck: true });
            view?.dispatch({ effects: recheckSpelling.of(null) });
            notify(`${dictionary.label} dictionary installed`);
          } catch (e) {
            notify(`Couldn't install ${dictionary.label}: ${e instanceof Error ? e.message : String(e)}`);
          }
        },
      });
      return result;
    });
    // Static language commands already cover some canonical tags (en-US, en-GB, ru, …).
    const fresh = commands.filter((c) => !registry.get(c.id) && !registeredDictionaryCommands.has(c.id));
    for (const c of fresh) registeredDictionaryCommands.add(c.id);
    if (fresh.length) registry.register(fresh);
  };
  const refreshDictionaries = async () => {
    try {
      const list = await platform.spellDictionaries();
      setDictionaries(list);
      registerDictionaryCommands(list);
    } catch { setDictionaries([]); }
  };
  void refreshDictionaries();
  const spellLangArg = () => (settings.spellLang === 'auto' ? undefined : settings.spellLang);
  const spellFor = () => {
    const ours = settings.spellcheck && osSpelling();
    const key = `${settings.spellcheck}|${settings.spellLang}|${ours}`;
    let ext = spellCache.get(key);
    if (!ext) {
      const lang = spellLangArg();
      ext = [
        EditorView.contentAttributes.of({
          spellcheck: settings.spellcheck && !ours ? 'true' : 'false',
          ...(lang ? { lang } : {}),
        }),
        ours ? spelling({ check: (text) => platform.spellCheckText(text, lang) }) : [],
      ];
      spellCache.set(key, ext);
    }
    return ext;
  };
  void platform.spellCheckText('', undefined).then((r) => setOsSpelling(r !== null), () => setOsSpelling(false));

  /** Brings the active editor state's per-window configuration up to date. */
  function applyLiveMode() {
    if (!view) return;
    const effects: StateEffect<unknown>[] = [];
    const want = liveFor(mode());
    if (liveCompartment.get(view.state) !== want) effects.push(liveCompartment.reconfigure(want));
    const language = languageFor();
    if (languageCompartment.get(view.state) !== language) effects.push(languageCompartment.reconfigure(language));
    const spell = spellFor();
    if (spellCompartment.get(view.state) !== spell) effects.push(spellCompartment.reconfigure(spell));
    if (effects.length) view.dispatch({ effects });
  }
  createEffect(on(() => [settings.spellcheck, settings.spellLang, osSpelling()], () => applyLiveMode(), { defer: true }));

  const docs = createDocuments({
    platform,
    makeState: (text) =>
      EditorState.create({ doc: text, extensions: [extensions, liveCompartment.of(liveFor(mode())), spellCompartment.of(spellFor())] }),
    confirm,
    notify,
    draftOwner,
    beforeSaveAs: (text, from, to) => assets.relocate(text, from, to),
    onClosing: (path, opened, current) => captureOnClose(path, opened, current),
    onExternalEdit: (path, before, after) => captureOnMcpEdit(path, before, after),
  });
  const store = docs.store;
  const activeDoc = createMemo(() => store.docs.find((d) => d.id === store.activeId));
  const menu = createContextMenu();

  /** Mirrors the live webview state into Rust so MCP tools can answer without a webview round trip. */
  function workspaceSnapshot(): WorkspaceSnapshot {
    return {
      root: explorer.state.root,
      documents: store.docs
        .filter((d) => !!d.path)
        .map((d) => ({
          path: d.path!,
          text: docs.textOf(d.id),
          eol: d.eol,
          bom: d.bom,
          mtimeMs: d.mtimeMs,
          dirty: docs.isDirty(d.id),
          conflict: d.conflict,
        })),
      activeDocument: activeDoc()?.path ?? null,
    };
  }
  // Rust-originated events temporarily suppress full snapshots. A rename/rewrite is delivered as
  // several events; echoing the view between them would overwrite Rust's newer mirror with stale text.
  let workspaceEventsUntil = 0;
  function syncWorkspace() {
    if (Date.now() < workspaceEventsUntil) return;
    void platform.workspaceSync(workspaceSnapshot()).catch(() => undefined);
  }

  const assets = createAssets({
    platform,
    mode: () => settings.imageFolder,
    root: () => explorer.state.root,
    notify,
  });

  const explorer = createExplorer({
    platform,
    confirm,
    notify,
    onRenamed: async (from, to) => {
      docs.renamePaths(from, to);
      await history.rename(from, to).catch(() => undefined);
      setHistoryRev((n) => n + 1);
      await rewriteLinksAfterRename(from, to);
    },
    onTrashed: (path) => docs.closeWithin(path),
    openFile: (path) => void docs.openPaths([path]),
    onShowAllFiles: (on) => update({ showAllFiles: on }),
  });

  // The Rust workspace is the live state MCP reads. Keep it in step with the view.
  createEffect(
    on(
      // Structural changes only. Text, mtime and status changes are sent by onUpdate upserts;
      // echoing them here can overwrite a newer Rust mirror while content events are queued.
      () => [explorer.state.root, store.docs.map((d) => `${d.id}:${d.path}`).join('|'), store.activeId] as const,
      () => syncWorkspace(),
      { defer: false },
    ),
  );
  // ---------- Sidebar and outline ----------
  const [sidebar, setSidebar] = createStore<Session['sidebar']>({ open: true, view: 'files', width: 248 });
  const [headings, setHeadings] = createSignal<Heading[]>([]);
  const [focusLine, setFocusLine] = createSignal(0);
  const activeHeading = createMemo(() => headingIndexAt(headings(), focusLine()));
  let outlineTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleOutline(state: EditorState, delay = 120) {
    clearTimeout(outlineTimer);
    outlineTimer = setTimeout(() => setHeadings(extractHeadings(state.doc.toString())), state.doc.length < 50_000 ? 0 : delay);
  }

  function showSidebar(v?: SidebarView) {
    if (v && sidebar.open && sidebar.view !== v) setSidebar('view', v);
    else if (v && !sidebar.open) setSidebar({ open: true, view: v });
    else setSidebar('open', !sidebar.open);
  }

  /** Jump to a line: caret and scroll in editor modes, scroll only in Preview. */
  function gotoLine(line: number) {
    if (!view || !preview) return;
    if (mode() === 'preview') {
      preview.sync.scrollToLine('preview', line);
      setFocusLine(line);
      return;
    }
    const pos = view.state.doc.line(Math.min(line + 1, view.state.doc.lines)).from;
    view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: 'start', yMargin: 32 }) });
    view.focus();
  }

  /** Links clicked in live mode or from menus. Anchors jump to the matching heading. */
  function followLink(href: string) {
    openLink(href, {
      platform,
      docPath: activeDoc()?.path ?? null,
      openPath: (p, anchor) =>
        void docs.openPaths([p]).then(() => {
          if (!anchor) return;
          // The note is the active tab once it's open; find the heading in its text.
          setTimeout(() => {
            const id = store.activeId;
            const line = id ? headingLine(docs.textOf(id), anchor) : null;
            if (line !== null) gotoLine(line);
          }, 60);
        }),
      openFile: (p) => openFileTab(p, true),
      openWiki: (t) => void openWiki(t),
      gotoAnchor: (id) => {
        const h = headings().find((x) => slugify(x.text) === id);
        if (h) gotoLine(h.line);
      },
    });
  }

  // ---------- Closed tabs (Reopen Closed Tab) ----------
  const closedPaths: string[] = [];
  let lastPaths = new Map<string, string | null>();
  createEffect(() => {
    const now = new Map(store.docs.map((d) => [d.id, d.path] as const));
    for (const [id, path] of lastPaths) if (!now.has(id) && path) closedPaths.push(path);
    if (closedPaths.length > 30) closedPaths.splice(0, closedPaths.length - 30);
    lastPaths = now;
  });
  function reopenClosed() {
    const p = closedPaths.pop();
    if (p) void docs.openPaths([p]);
    else notify('No recently closed tabs');
  }

  /** Expands the active file's folders in the explorer and shows it. */
  async function revealActive() {
    const p = activeDoc()?.path;
    const root = explorer.state.root;
    if (!p) return notify('This document isn’t saved to a file yet');
    if (!root || !isWithin(p, root)) return notify('The file isn’t inside the open folder');
    const chain: string[] = [];
    for (let d = dirname(p); d && isWithin(d, root) && !samePath(d, root); d = dirname(d)) chain.unshift(d);
    for (const d of chain) await explorer.toggle(d, true);
    setSidebar({ open: true, view: 'files' });
    requestAnimationFrame(() => {
      const row = [...document.querySelectorAll<HTMLElement>('.tree-row[data-path]')].find(
        (r) => samePath(r.dataset.path!, p) && !r.closest('.tree-section')?.querySelector('.section-title')?.textContent?.includes('Favorites'),
      );
      row?.scrollIntoView({ block: 'center' });
      row?.focus();
    });
  }

  function cycleTab(dir: 1 | -1) {
    const list = store.docs;
    if (list.length < 2) return;
    const i = list.findIndex((d) => d.id === store.activeId);
    docs.activate(list[(i + dir + list.length) % list.length].id);
  }

  const openPalette = (kind: 'commands' | 'files' | 'line' | 'headings') =>
    setPalette(kind === 'commands' ? '>' : kind === 'line' ? ':' : kind === 'headings' ? '@' : '');

  registry.register(
    buildCommands({
      docs,
      explorer,
      settings,
      update,
      view: () => view,
      mode,
      setMode,
      cycleMode,
      showSidebar,
      openPalette,
      customizeToolbar: () => setCustomizing(true),
      cycleTab,
      closeFileTab: () => {
        const id = activeFile();
        if (id) closeFileTab(id);
        return !!id;
      },
      reopenClosed,
      revealActive: () => void revealActive(),
      notify,
      pickColorTheme,
      clipboard: { read: () => platform.readClipboard(), write: (t) => platform.writeClipboard(t) },
      pickCustomColor: (kind, apply) => pickCustomColor(kind, apply),
    }),
  );

  /** Pasting a lone video link on its own line offers to embed it, quietly in the status bar. */
  function offerEmbed(u: ViewUpdate) {
    if (!u.transactions.some((tr) => tr.isUserEvent('input.paste'))) return;
    let pasted = '';
    let at = -1;
    u.changes.iterChanges((_fa, _ta, fromB, _tb, inserted) => {
      pasted = inserted.toString();
      at = fromB;
    });
    const url = pasted.trim();
    if (at < 0 || !isEmbeddableUrl(url)) return;
    const line = u.state.doc.lineAt(at);
    if (line.text.trim() !== url) return;
    notify('Embed this video?', {
      label: 'Embed',
      run: () => {
        if (!view) return;
        const l = view.state.doc.lineAt(Math.min(line.from, view.state.doc.length));
        if (l.text.trim() !== url) return; // the line changed since
        const insert = `![](${url})`;
        view.dispatch({ changes: { from: l.from, to: l.to, insert }, selection: { anchor: l.from + 2 }, userEvent: 'input.format' });
        view.focus();
        setMessage(null);
      },
    });
  }

  registry.register([
    { id: 'edit.spellcheck', title: 'Toggle Spellcheck', category: 'Spelling', run: () => update({ spellcheck: !settings.spellcheck }) },
    ...SPELL_LANGUAGES.map(([code, label]) => ({
      id: `spell.${code}`,
      title: `Spelling Language: ${label}`,
      category: 'Spelling',
      run: () => {
        update({ spellLang: code, spellcheck: true });
        if (code !== 'auto') notify(`Spellcheck: ${label}. Built-in dictionaries work offline; optional ones can be downloaded from this menu.`);
      },
    })),
    { id: 'window.new', title: 'New Window', category: 'Window', keys: 'Mod+Shift+N', run: () => platform.openWindow(null) },
    { id: 'space.switch', title: 'Switch Space…', category: 'Space', keys: 'Mod+Alt+O', run: () => setPalette('#') },
    { id: 'space.save', title: 'Save Window as Space…', category: 'Space', run: saveAsSpace },
    { id: 'space.rename', title: 'Rename Space…', category: 'Space', when: () => !!space(), run: renameSpace },
    { id: 'space.close', title: 'Close Space', category: 'Space', when: () => !!space(), run: closeSpace },
    { id: 'space.delete', title: 'Delete Space…', category: 'Space', when: () => !!space(), run: () => deleteSpace() },
  ]);

  // ---------- Themes ----------
  const [userThemes, setUserThemes] = createSignal<UserTheme[]>([]);
  let themesFolder: string | null = null;
  const currentUserTheme = () => userThemes().find((t) => t.id === settings.previewTheme) ?? null;
  /** The built-in style that drives typography (a user theme builds on one). */
  const activeBase = (): BaseTheme => {
    const id = settings.previewTheme;
    if (isBuiltinTheme(id)) return id;
    return currentUserTheme()?.base ?? 'paper';
  };
  async function reloadThemes() {
    try {
      const { dir, themes } = await loadUserThemes(platform);
      themesFolder = dir;
      setUserThemes(themes);
      registry.replace(
        'theme.user.',
        themes.map((t) => ({
          id: `theme.user.${t.id}`,
          title: `Preview Style: ${t.name}`,
          category: 'Appearance',
          run: () => update({ previewTheme: t.id as PreviewTheme }),
        })),
      );
    } catch {
      setUserThemes([]);
    }
  }
  async function openThemesFolder() {
    const dir = themesFolder ?? (await platform.themesDir());
    await platform.revealPath(joinPath(dir, (userThemes()[0] && basename(userThemes()[0].path)) || '')).catch(() => platform.revealPath(dir));
  }
  // User theme CSS is injected after the built-in styles, so it applies to the preview and live mode.
  const themeStyle = document.createElement('style');
  themeStyle.id = 'calmmd-user-theme';
  document.head.append(themeStyle);
  createEffect(() => {
    themeStyle.textContent = currentUserTheme()?.css ?? '';
    requestAnimationFrame(() => {
      view?.requestMeasure();
      preview?.sync.refresh();
    });
  });

  // ---------- Export ----------
  const exporter = createExporter({
    platform,
    current: () => {
      const d = activeDoc();
      return d && view ? { text: view.state.doc.toString(), path: d.path, name: docName(d) } : null;
    },
    theme: (id) => {
      const themeId = id ?? settings.previewTheme;
      if (isBuiltinTheme(themeId)) return { base: themeId, css: null };
      const t = userThemes().find((x) => x.id === themeId);
      return t ? { base: t.base, css: t.css } : { base: 'paper', css: null };
    },
    notify,
  });
  const [exportDialog, setExportDialog] = createSignal<{ kind: 'pdf' | 'html'; prefs: ExportPrefs; engine: string | null } | null>(null);
  const engineName = (path: string | null) => {
    if (!path) return null;
    const p = path.toLowerCase();
    return p.includes('edge') ? 'Microsoft Edge' : p.includes('brave') ? 'Brave' : p.includes('chromium') ? 'Chromium' : 'Google Chrome';
  };
  async function openExport(kind: 'pdf' | 'html') {
    if (!activeDoc()) return;
    const [prefs, engine] = await Promise.all([exporter.loadPrefs(), kind === 'pdf' ? platform.findBrowser().catch(() => null) : null]);
    setExportDialog({ kind, prefs, engine: engineName(engine) });
  }
  const [pandoc, setPandoc] = createSignal(false);
  // The MCP bridge needs all four controllers, so it is installed after the exporter exists.
  createMcpBridge({ platform, docs, explorer });

  registry.register([
    { id: 'export.pdf', title: 'Export as PDF…', category: 'File', run: () => openExport('pdf') },
    { id: 'export.html', title: 'Export as HTML…', category: 'File', run: () => openExport('html') },
    { id: 'export.copyHtml', title: 'Copy as HTML', category: 'Edit', run: () => exporter.copyHtml() },
    { id: 'export.docx', title: 'Export as Word (Pandoc)…', category: 'File', when: pandoc, run: () => exporter.exportPandoc('docx') },
    { id: 'export.odt', title: 'Export as OpenDocument (Pandoc)…', category: 'File', when: pandoc, run: () => exporter.exportPandoc('odt') },
    { id: 'export.epub', title: 'Export as EPUB (Pandoc)…', category: 'File', when: pandoc, run: () => exporter.exportPandoc('epub') },
    {
      id: 'mcp.start',
      title: 'Start MCP Server',
      category: 'MCP',
      when: () => platform.kind === 'tauri',
      run: async () => {
        const s = await platform.mcpStart();
        setMcpInfo(s);
        if (!s.url) return notify('Could not start the MCP server');
        await platform.writeClipboard(`${s.url} · token: ${s.token ?? ''}`).catch(() => undefined);
        notify(`MCP server: ${s.url} (connection copied)`);
      },
    },
    {
      id: 'mcp.stop',
      title: 'Stop MCP Server',
      category: 'MCP',
      when: () => platform.kind === 'tauri',
      run: async () => {
        setMcpInfo(await platform.mcpStop());
        notify('MCP server stopped');
      },
    },
    {
      id: 'mcp.autostart',
      title: settings.mcpAutostart ? 'MCP Server: Don’t Auto-start' : 'MCP Server: Auto-start',
      category: 'MCP',
      when: () => platform.kind === 'tauri',
      run: () => {
        const on = !settings.mcpAutostart;
        update({ mcpAutostart: on });
        notify(on ? 'CalmMD will start the MCP server on launch' : 'MCP server won’t auto-start');
      },
    },
    { id: 'theme.openFolder', title: 'Open Themes Folder…', category: 'Appearance', run: openThemesFolder },
    { id: 'theme.reload', title: 'Reload Themes', category: 'Appearance', run: () => reloadThemes().then(() => notify('Themes reloaded')) },
  ]);

  registry.register([
    {
      id: 'insert.imageFile',
      title: 'Image from File…',
      category: 'Insert',
      when: () => showsEditor(mode()),
      run: async () => {
        const paths = await platform.openImageDialog();
        if (paths.length) await insertImagePaths(paths);
      },
    },
    { id: 'images.document', title: 'Save Pasted Images Next to the Document', category: 'Images', run: () => update({ imageFolder: 'document' }) },
    { id: 'images.root', title: 'Save Pasted Images in the Folder Root', category: 'Images', run: () => update({ imageFolder: 'root' }) },
  ]);

  // ---------- Search across the folder ----------
  const [searchFocus, setSearchFocus] = createSignal<{ n: number; query?: string }>({ n: 0 });
  /** Opens a file and puts the cursor at the start of a line (0-based). */
  async function openAtLine(path: string, line: number) {
    await docs.openPaths([path]);
    // The tab switch swaps the editor state on the next tick.
    setTimeout(() => gotoLine(line), 30);
  }
  registry.register([
    {
      id: 'view.search',
      title: 'Search in Folder',
      category: 'View',
      keys: 'Mod+Shift+F',
      run: () => {
        const sel = view?.state.selection.main;
        const picked = sel && !sel.empty && sel.to - sel.from < 120 ? view!.state.sliceDoc(sel.from, sel.to) : undefined;
        setSidebar({ open: true, view: 'search' });
        setSearchFocus({ n: searchFocus().n + 1, query: picked && !picked.includes('\n') ? picked : undefined });
      },
    },
  ]);

  // ---------- Custom colours ----------
  /** The system colour picker, starting from the colour in use. */
  function pickCustomColor(kind: 'highlight' | 'text', apply: (hex: string) => void) {
    const input = document.createElement('input');
    input.type = 'color';
    const current = kind === 'highlight' ? settings.highlightColor : settings.textColor;
    input.value = /^#[0-9a-f]{6}$/i.test(current) ? current : kind === 'highlight' ? '#facc15' : '#e5484d';
    input.style.cssText = 'position:fixed;left:-100px;top:0;opacity:0';
    document.body.append(input);
    input.addEventListener('change', () => {
      apply(input.value);
      input.remove();
    });
    input.addEventListener('blur', () => setTimeout(() => input.remove(), 1000));
    input.click();
  }

  // ---------- File history ----------
  const history = createHistory({ platform });
  const [historyRev, setHistoryRev] = createSignal(0);
  const [versionView, setVersionView] = createSignal<{ path: string; version: Version; text: string } | null>(null);

  /** After a rename, retarget links in every note so folders can be reorganised safely. */
  async function rewriteLinksAfterRename(from: string, to: string) {
    if (!explorer.state.root) return;
    const files = (await explorer.everyFile()).filter(isMarkdownPath);
    let changed = 0;
    let failed = 0;
    for (const path of files) {
      const open = store.docs.find((d) => d.path && samePath(d.path, path));
      const before = open ? docs.textOf(open.id) : (await platform.readTextFile(path).catch(() => null))?.text;
      if (before === undefined) continue;
      const after = rewriteLinks(before, path, from, to, files);
      if (after === before) continue;
      try {
        if (open) await docs.setContentByPath(path, after, { mode: 'replace', flush: true });
        else {
          await history.save(path, before);
          await platform.writeTextFile(path, after);
          await history.save(path, after);
        }
        changed++;
      } catch {
        failed++;
      }
    }
    setHistoryRev((n) => n + 1);
    if (failed) notify(`Updated links in ${changed} notes; ${failed} failed`);
    else if (changed) notify(`Updated links in ${changed} note${changed === 1 ? '' : 's'}`);
  }

  /** An MCP edit: keep the previous text and the result, so an agent change is reversible. */
  async function captureOnMcpEdit(path: string, before: string, after: string) {
    if (before.trim()) await history.save(path, before);
    await history.save(path, after);
    setHistoryRev((n) => n + 1);
  }

  platform.onWorkspaceEvent((event: WorkspaceEvent) => {
    workspaceEventsUntil = Date.now() + 2000;
    if (event.kind === 'content' && event.path && event.text !== undefined && event.mtimeMs !== undefined) {
      // Rust already persisted this text and updated its mirror. Do not echo a full snapshot
      // back here: other content events may still be queued and would overwrite newer text.
      void docs.applyWorkspaceContent(event.path, event.text, event.mtimeMs).then(() => {
        setHistoryRev((n) => n + 1);
      });
    } else if (event.kind === 'renamed' && event.from && event.to) {
      docs.renamePaths(event.from, event.to);
      void explorer.externalRenamed(event.from, event.to);
      setHistoryRev((n) => n + 1);
    } else if (event.kind === 'created' && event.path) {
      const parent = dirname(event.path);
      if (parent) void explorer.refreshDir(parent);
      // Do not open the file here: a following content event is authoritative and opens it with the final text.
    } else if (event.kind === 'trashed' && event.path) {
      docs.closeWithin(event.path);
      void explorer.externalTrashed(event.path, event.isDir === true);
    }
  });

  /** A changed file closed: keep how it was when opened (if that's new) and how it is now. */
  async function captureOnClose(path: string, opened: string, current: string) {
    if (!settings.historyOnClose) return;
    if (opened.trim()) await history.save(path, opened);
    await history.save(path, current);
    setHistoryRev((n) => n + 1);
  }

  async function saveVersion() {
    const d = activeDoc();
    if (!d || !view) return;
    if (!d.path) {
      notify('Save this document to a file first');
      return;
    }
    const path = d.path;
    const name = await prompt({
      title: 'Save Version',
      message: 'Keep the current text in this file’s history. A name is optional; named versions are never removed automatically.',
      placeholder: 'For example: Sent to Sam',
      primary: 'Save Version',
      allowEmpty: true,
    });
    if (name === null) return;
    const v = await history.save(path, view.state.doc.toString(), name || null).catch((e) => {
      notify(`Couldn’t save the version: ${e instanceof Error ? e.message : String(e)}`);
      return undefined;
    });
    if (v === undefined) return;
    setHistoryRev((n) => n + 1);
    notify(v ? 'Version saved' : 'Nothing changed since the last version');
  }

  async function openVersion(path: string, version: Version) {
    const text = await history.read(path, version.id);
    if (text === null) {
      notify('That version couldn’t be read');
      setHistoryRev((n) => n + 1);
      return;
    }
    setVersionView({ path, version, text });
  }

  async function restoreVersion() {
    const vv = versionView();
    const d = activeDoc();
    if (!vv || !view || !d?.path || !samePath(d.path, vv.path)) return;
    const current = view.state.doc.toString();
    // Nothing is lost: the text being replaced becomes a version first.
    await history.save(vv.path, current);
    const change = minimalChange(current, vv.text);
    if (change) view.dispatch({ changes: change, userEvent: 'input.restore', scrollIntoView: true });
    setVersionView(null);
    setHistoryRev((n) => n + 1);
    notify(`Restored the version from ${versionTime(vv.version.savedAt)}. Undo brings your text back.`);
    view.focus();
  }

  async function deleteVersion() {
    const vv = versionView();
    if (!vv) return;
    const choice = await confirm({
      title: 'Delete this version?',
      message: `“${vv.version.name ?? versionTime(vv.version.savedAt)}” will be removed from this file’s history. This can’t be undone.`,
      primary: 'Delete Version',
      destructive: 'primary',
    });
    if (choice !== 'primary') return;
    await history.remove(vv.path, vv.version.id);
    setVersionView(null);
    setHistoryRev((n) => n + 1);
  }

  registry.register([
    { id: 'history.save', title: 'Save Version…', category: 'File', run: () => void saveVersion() },
    { id: 'history.show', title: 'Show File History', category: 'File', run: () => setSidebar({ open: true, view: 'history' }) },
    { id: 'history.clearAll', title: 'Clear All File History…', category: 'File', run: () => void clearAllHistory() },
    { id: 'help.checkUpdates', title: 'Check for Updates…', category: 'Help', run: () => void checkForUpdates(true) },
    {
      id: 'help.updateNotices',
      title: 'Tell Me About New Versions',
      category: 'Help',
      run: () => {
        const on = settings.updateCheck !== 'on';
        update({ updateCheck: on ? 'on' : 'off' });
        notify(on ? 'CalmMD will tell you when a new version is out' : 'No more new-version notices');
        if (on) void checkForUpdates();
      },
    },
    { id: 'insert.drawing', title: 'Drawing…', category: 'Insert', run: () => void newDrawing(true) },
    { id: 'file.newDrawing', title: 'New Drawing…', category: 'File', run: () => void newDrawing(false) },
  ]);

  async function clearFileHistory(path: string) {
    const c = await confirm({
      title: 'Delete this file’s history?',
      message: `Every saved version of “${basename(path)}” goes. The file itself isn’t touched. This can’t be undone.`,
      primary: 'Delete History',
      destructive: 'primary',
    });
    if (c !== 'primary') return;
    await history.clearFile(path).catch((e) => notify(String(e)));
    setHistoryRev((n) => n + 1);
  }
  async function clearAllHistory() {
    const u = await history.usage().catch(() => null);
    if (!u?.versions) return notify('There’s no file history to clear');
    const c = await confirm({
      title: 'Clear all file history?',
      message: `${u.versions === 1 ? 'The one saved version' : `All ${u.versions.toLocaleString()} saved versions`} of ${u.files.toLocaleString()} ${u.files === 1 ? 'file' : 'files'} will be deleted. Your files themselves aren’t touched. This can’t be undone.`,
      primary: 'Clear All',
      destructive: 'primary',
    });
    if (c !== 'primary') return;
    await history.clearAll().catch((e) => notify(String(e)));
    setHistoryRev((n) => n + 1);
    notify('File history cleared');
  }

  // ---------- New-version notices ----------
  // Only with the user's OK (asked once). About once a day the app asks GitHub for the latest release.
  async function checkForUpdates(manual = false) {
    if (!manual && settings.updateCheck !== 'on') return;
    if (!manual) {
      const last = Number(await platform.kvGet('updates/lastCheck').catch(() => null)) || null;
      if (!isCheckDue(last, Date.now())) return;
    }
    let latest: Release | null = null;
    try {
      const r = await fetch(RELEASES_API, { headers: { Accept: 'application/vnd.github+json' } });
      latest = r.ok ? parseRelease(await r.json()) : null;
    } catch {
      if (manual) notify('Couldn’t reach GitHub to check for a new version');
      return;
    }
    await platform.kvSet('updates/lastCheck', String(Date.now()));
    const shown = await platform.kvGet('updates/shown').catch(() => null);
    const newer = !!latest && compareVersions(latest.version, __APP_VERSION__) > 0;
    if (latest && newer && (manual || shouldAnnounce(latest, __APP_VERSION__, shown))) {
      const release = latest;
      await platform.kvSet('updates/shown', release.version);
      notify(`CalmMD ${release.version} is out`, { label: 'What’s New', run: () => void platform.openExternal(release.url) }, true);
    } else if (manual) notify(`You have the latest version (${__APP_VERSION__})`);
  }
  async function askAboutUpdates() {
    if (settings.updateCheck !== 'ask') return;
    const c = await confirm({
      title: 'Hear about new versions?',
      message:
        'CalmMD can check GitHub about once a day and show a short note when a new version is out. Nothing about you or your files is sent. You can change this any time in the Help menu.',
      primary: 'Yes, Tell Me',
      secondary: 'No Thanks',
    });
    if (c === 'cancel') return; // asked again next launch
    update({ updateCheck: c === 'primary' ? 'on' : 'off' });
  }

  // ---------- Share bundles ----------
  const dataUrlBytes = (url: string) => {
    const b64 = url.slice(url.indexOf(',') + 1);
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  };
  async function shareBundle(kind: 'zip' | 'single') {
    const d = activeDoc();
    if (!d || !view) return;
    await docs.flushAll();
    const text = view.state.doc.toString();
    const name = (d.path ? stem(d.path) : docName(d).replace(/\.md$/i, '')) || 'Document';
    const base = d.path ? joinPath(dirname(d.path), name) : name;
    try {
      if (kind === 'zip') {
        const plan = planZipBundle(text, d.path);
        const out = await platform.saveFileDialog(`${base}.zip`, 'zip');
        if (!out) return;
        const entries = [{ name: `${name}.md`, data: new TextEncoder().encode(plan.text) }];
        const missing: string[] = [];
        for (const f of plan.files) {
          const url = await platform.readImageDataUrl(f.from).catch(() => null);
          if (url && url.includes(';base64,')) entries.push({ name: f.name, data: dataUrlBytes(url) });
          else missing.push(basename(f.from));
        }
        await platform.writeBinaryFile(out, createZip(entries));
        notify(missing.length ? `Saved ${basename(out)} (missing: ${missing.join(', ')})` : `Saved ${basename(out)}`, { label: 'Show', run: () => void platform.revealPath(out) });
      } else {
        const out = await platform.saveFileDialog(`${base} (with images).md`, 'markdown');
        if (!out) return;
        const urls = new Map<string, string>();
        for (const p of localImages(text, d.path)) {
          const url = await platform.readImageDataUrl(p).catch(() => null);
          if (url) urls.set(p, url);
        }
        const r = inlineImages(text, d.path, urls);
        await platform.writeTextFile(out, r.text);
        notify(r.missing.length ? `Saved ${basename(out)} (missing: ${r.missing.join(', ')})` : `Saved ${basename(out)}`, { label: 'Show', run: () => void platform.revealPath(out) });
      }
    } catch (e) {
      notify(`Couldn't share: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
  registry.register([
    { id: 'share.zip', title: 'Share as .zip with Images…', category: 'File', when: () => !!activeDoc(), run: () => shareBundle('zip') },
    { id: 'share.single', title: 'Share as One File (Images Inside)…', category: 'File', when: () => !!activeDoc(), run: () => shareBundle('single') },
  ]);

  // ---------- Help ----------
  const [helpDialog, setHelpDialog] = createSignal<'shortcuts' | 'about' | null>(null);
  registry.register([
    { id: 'help.shortcuts', title: 'Keyboard Shortcuts', category: 'Help', keys: 'F1', run: () => setHelpDialog('shortcuts') },
    { id: 'help.welcome', title: 'Welcome Guide', category: 'Help', run: () => docs.newDoc(welcomeText) },
    { id: 'help.markdown', title: 'Markdown Guide', category: 'Help', run: () => docs.newDoc(guideText) },
    { id: 'help.about', title: `About ${BRAND.name}`, category: 'Help', run: () => setHelpDialog('about') },
    { id: 'app.quit', title: 'Close Window', category: 'Window', palette: false, run: () => platform.closeWindow() },
  ]);

  // ---------- Menus ----------
  const menus: TopMenu[] = appMenus({
    settings,
    mode,
    effectiveScheme,
    pandoc: () => pandoc(),
    inSpace: () => !!space(),
    spaces: () => recentSpaces().map((s) => ({ id: s.id, name: s.name, current: s.id === space()?.id })),
    switchSpace: (id) => void switchSpace(id),
    recentFolders: () => explorer.state.recent,
    openFolder: (p) => void explorer.setRoot(p),
    clearRecentFolders: () => void explorer.forgetRecent(),
    userThemes: () => userThemes().map((t) => ({ id: t.id, name: t.name })),
    pickColorTheme,
    dictionaries,
    isMac,
  });
  const resolveTop = (m: TopMenu) => resolveMenu(m.items(), registry, hint);

  // macOS: the same menus as the native app menu, rebuilt (debounced) when anything they show changes.
  if (platform.nativeMenu) {
    const actions = new Map<string, () => void>();
    let seq = 0;
    const toNative = (entries: ResolvedEntry[]): NativeMenuItem[] =>
      entries.map((e): NativeMenuItem => {
        if (e.kind === 'separator') return e;
        if (e.kind === 'submenu') return { kind: 'submenu', label: e.label, enabled: e.enabled, items: toNative(e.items()) };
        // The OS's own Cut/Copy/Paste/Select All, or the keys don't reach the webview.
        const predefined = e.id ? NATIVE_EDIT_ITEMS[e.id] : undefined;
        if (predefined) return { kind: 'predefined', item: predefined };
        const id = `m${seq++}`;
        actions.set(id, e.run);
        // No accelerators: the webview already handles every shortcut (registry and CodeMirror), and a
        // native key equivalent on top could run a toggle twice. The menu shows the keys in the label.
        const keys = e.keys ? `   ${formatBinding(e.keys, true)}` : '';
        return { kind: 'item', id, label: `${e.label}${keys}`, enabled: e.enabled, checked: e.checked };
      });
    let menuTimer: ReturnType<typeof setTimeout> | undefined;
    createEffect(() => {
      actions.clear();
      seq = 0;
      const native: NativeMenu[] = menus.map((m) => ({ label: m.label, items: toNative(resolveTop(m)) }));
      clearTimeout(menuTimer);
      menuTimer = setTimeout(() => void platform.setAppMenu(native, (id) => actions.get(id)?.()).catch(() => undefined), 120);
    });
  }

  // ---------- Palette sources ----------
  const [allFiles, setAllFiles] = createSignal<string[]>([]);
  createEffect(
    on(palette, (q) => {
      if (q !== null) {
        void explorer.allFiles().then(setAllFiles);
        void refreshSpaces();
      }
    }),
  );
  const paletteSources: Record<PalettePrefix, PaletteSource> = {
    '>': {
      placeholder: 'Type a command',
      items: () =>
        registry.forPalette().map((c) => ({
          id: c.id,
          label: `${c.category}: ${c.title}`,
          hint: hint(c.id),
          run: () => registry.run(c.id),
        })),
    },
    '': {
      placeholder: 'Go to file (type > for commands)',
      empty: explorer.state.root ? 'No matching files' : 'Open a folder to search its files',
      items: () => {
        const root = explorer.state.root;
        const seen = new Set<string>();
        const out: PaletteItem[] = [];
        const add = (path: string, detail?: string) => {
          const key = path.toLowerCase();
          if (seen.has(key)) return;
          seen.add(key);
          const dir = dirname(path);
          const rel = root && isWithin(dir, root) ? dir.slice(root.length).replace(/^[\\/]/, '') : dir;
          out.push({ id: path, label: basename(path), detail: detail ?? rel, run: () => void docs.openPaths([path]) });
        };
        for (const d of store.docs) if (d.path) add(d.path, 'open');
        for (const f of explorer.state.favorites) if (!f.isDir) add(f.path, 'favorite');
        for (const p of allFiles()) add(p);
        return out;
      },
    },
    ':': {
      placeholder: 'Line number',
      raw: true,
      items: (q) => {
        const lines = view?.state.doc.lines ?? 1;
        const n = parseInt(q, 10);
        if (!q.trim()) return [{ id: 'line', label: `Type a line number (1–${lines})`, run: () => {} }];
        if (!Number.isFinite(n)) return [];
        const line = Math.min(Math.max(1, n), lines);
        return [{ id: 'line', label: `Go to line ${line}`, run: () => gotoLine(line - 1) }];
      },
    },
    '#': {
      placeholder: 'Switch to a Space (Ctrl/⌘+Enter opens it in a new window)',
      empty: 'No Spaces yet. Use “Save Window as Space…” to create one.',
      items: () => [
        ...recentSpaces().map((s) => ({
          id: s.id,
          label: s.name,
          detail: s.id === space()?.id ? 'this window' : s.session.root ? basename(s.session.root) : undefined,
          run: () => switchSpace(s.id),
          altRun: () => openSpaceInNewWindow(s.id),
        })),
        { id: '__new', label: 'Save Window as Space…', detail: 'new', run: saveAsSpace },
      ],
    },
    '@': {
      placeholder: 'Heading',
      empty: 'No headings in this document',
      items: () =>
        headings().map((h, i) => ({
          id: `h${i}`,
          label: h.text || 'Untitled section',
          detail: `H${h.level}`,
          run: () => gotoLine(h.line),
        })),
    },
  };

  // ---------- Session ----------
  // Where this window's session lives: in its Space, or for a loose main window in "session".
  // Extra loose windows are scratch windows, and their layout isn't kept.
  let sessionReady = false;
  let sessionTimer: ReturnType<typeof setTimeout> | undefined;
  const buildSession = (): Session => ({
    v: 1,
    ...docs.snapshot(),
    root: explorer.state.root,
    expanded: explorer.expandedList(),
    sidebar: { ...sidebar },
    viewMode: chosenMode(),
  });
  const flushSession = async () => {
    clearTimeout(sessionTimer);
    if (!sessionReady) return;
    const s = space();
    if (s) await spaces.saveSession(s.id, buildSession());
    else if (isMain) await platform.sessionSet('session', buildSession());
  };

  /** Applies a session to this window: layout, folder, tabs. */
  async function loadSession(session: Session, launchPaths: string[] = []) {
    setSidebar(session.sidebar);
    if (session.viewMode && session.viewMode !== chosenMode()) {
      setModeSignal(session.viewMode);
      applyLiveMode();
    }
    await explorer.setRoot(session.root, session.expanded);
    // The main window adopts drafts left by scratch windows that are no longer open.
    const open = new Set((await platform.windowLabels().catch(() => [label])).map((l) => `window:${l}`));
    const me = draftOwner();
    await docs.restore(launchPaths, session, {
      adopt: (owner) =>
        owner === me || (isMain && !space() && (owner === undefined || (owner.startsWith('window:') && !open.has(owner)))),
    });
    if (preview && showsPreview(mode())) {
      preview.invalidate();
      preview.renderNow();
    }
  }

  /** Leaves the current Space or session (everything is saved) and clears the window. */
  async function leaveCurrent() {
    await docs.takeClosingVersions();
    await docs.flushAll();
    await flushSession();
    sessionReady = false; // nothing saves while the window is between sessions
    docs.reset();
  }

  async function switchSpace(id: string) {
    if (space()?.id === id) return;
    const holder = await platform.spaceWindow(id);
    if (holder && holder !== label) {
      await platform.focusWindow(holder);
      return;
    }
    const rec = await spaces.get(id);
    if (!rec) {
      notify('That Space no longer exists');
      void refreshSpaces();
      return;
    }
    await leaveCurrent();
    const other = await platform.claimSpace(id);
    if (other) {
      // Lost a race with another window: go back to a clean loose window.
      notify(`“${rec.name}” is already open in another window`);
      setSpace(null);
      await loadSession(parseSession(null));
      sessionReady = true;
      return;
    }
    setSpace({ id: rec.id, name: rec.name });
    await loadSession(rec.session);
    sessionReady = true;
    await flushSession(); // bumps lastOpened
    if (isMain) void platform.kvSet('main-space', rec.id);
    void refreshSpaces();
  }

  async function openSpaceInNewWindow(id: string) {
    const holder = await platform.spaceWindow(id);
    if (holder) return platform.focusWindow(holder);
    await platform.openWindow(id);
  }

  async function saveAsSpace() {
    const root = explorer.state.root;
    const name = await prompt({
      title: space() ? 'Save as a New Space' : 'Save Window as a Space',
      message: 'A Space remembers this folder, your open tabs, the layout and the view mode.',
      value: root ? basename(root) : 'My Space',
      primary: 'Save Space',
    });
    if (!name) return;
    await docs.flushAll();
    const rec = await spaces.create(name, buildSession());
    await platform.claimSpace(rec.id);
    setSpace({ id: rec.id, name: rec.name });
    await docs.rewriteDrafts(); // the drafts now belong to the Space
    if (isMain) void platform.kvSet('main-space', rec.id);
    await flushSession();
    void refreshSpaces();
    notify(`Saved Space “${rec.name}”`);
  }

  async function closeSpace() {
    if (!space()) return;
    await leaveCurrent();
    await platform.claimSpace(null);
    setSpace(null);
    if (isMain) void platform.kvSet('main-space', '');
    await loadSession(parseSession(null));
    sessionReady = true;
  }

  async function renameSpace() {
    const s = space();
    if (!s) return;
    const name = await prompt({ title: 'Rename Space', value: s.name, primary: 'Rename' });
    if (!name || name === s.name) return;
    await spaces.rename(s.id, name);
    setSpace({ ...s, name });
    void refreshSpaces();
  }

  async function deleteSpace(id = space()?.id) {
    if (!id) return;
    const rec = await spaces.get(id);
    if (!rec) return;
    const holder = await platform.spaceWindow(id);
    if (holder && holder !== label) return notify(`“${rec.name}” is open in another window. Close it there first.`);
    const choice = await confirm({
      title: `Delete the Space “${rec.name}”?`,
      message: 'Only the saved layout is removed. Your files and folders stay exactly where they are.',
      primary: 'Delete Space',
      destructive: 'primary',
    });
    if (choice !== 'primary') return;
    if (space()?.id === id) await closeSpace();
    await spaces.remove(id);
    void refreshSpaces();
  }

  /** The sidebar's Space switcher: every Space, then what can be done with this one. */
  function spaceMenuItems(): MenuItem[] {
    const list = recentSpaces();
    const current = space();
    return [
      ...(list.length
        ? [
            { heading: 'Spaces' } as MenuItem,
            ...list.slice(0, 12).map(
              (s): MenuItem => ({
                label: s.name,
                checked: s.id === current?.id,
                detail: s.session.root ? basename(s.session.root) : undefined,
                action: () => switchSpace(s.id),
              }),
            ),
            'separator' as const,
          ]
        : []),
      { label: current ? 'Save as New Space…' : 'Save Window as Space…', action: saveAsSpace },
      { label: 'Find a Space…', hint: hint('space.switch'), action: () => setPalette('#') },
      ...(current
        ? [
            'separator' as const,
            { label: 'Rename Space…', action: renameSpace },
            { label: 'Close Space', action: closeSpace },
            { label: 'Delete Space…', danger: true, action: () => deleteSpace() },
          ]
        : []),
    ];
  }
  const saveSession = () => {
    if (!sessionReady) return;
    clearTimeout(sessionTimer);
    sessionTimer = setTimeout(() => void flushSession(), 800);
  };
  createEffect(() => {
    // Track everything structural. Cursor and scroll changes call saveSession directly.
    store.docs.forEach((d) => d.path);
    store.activeId;
    explorer.state.root;
    Object.values(explorer.state.expanded);
    sidebar.open, sidebar.view, sidebar.width;
    saveSession();
  });

  // ---------- Selection bubble ----------
  const [bubble, setBubble] = createSignal<{ x: number; y: number } | null>(null);
  let pointerSelecting = false;
  let bubbleTimer: ReturnType<typeof setTimeout> | undefined;
  function placeBubble() {
    if (!view || mode() === 'preview') return setBubble(null);
    const sel = view.state.selection.main;
    if (sel.empty || !view.hasFocus) return setBubble(null);
    const a = view.coordsAtPos(sel.from);
    const b = view.coordsAtPos(sel.to);
    if (!a || !b) return setBubble(null);
    const sameLine = Math.abs(a.top - b.top) < 4;
    const x = sameLine ? (a.left + b.left) / 2 : a.left + 80;
    const y = Math.min(a.top, b.top);
    const width = 300;
    setBubble({ x: Math.max(8, Math.min(window.innerWidth - width - 8, x - width / 2)), y: Math.max(8, y - 44) });
  }

  function onUpdate(u: ViewUpdate) {
    const id = store.activeId;
    if (!id) return;
    docs.syncState(id, u.state);
    if (isUserEdit(u)) docs.touch(id);
    if (u.docChanged) {
      const meta = store.docs.find((d) => d.id === id);
      // Rust already wrote input.mcp transactions; the event handler sends the full snapshot.
      const fromWorkspace = u.transactions.some((tr) => tr.isUserEvent('input.mcp'));
      if (meta?.path && !fromWorkspace) {
        void platform
          .workspaceUpsert(
            {
              path: meta.path,
              text: u.state.doc.toString(),
              eol: meta.eol,
              bom: meta.bom,
              mtimeMs: meta.mtimeMs,
              dirty: docs.isDirty(id),
              conflict: meta.conflict,
            },
            activeDoc()?.path ?? null,
          )
          .catch(() => undefined);
      }
      scheduleStats(u.state);
      scheduleOutline(u.state);
      offerEmbed(u);
    }
    if (u.selectionSet || u.docChanged) {
      updateSelectionStats(u.state);
      setFocusLine(u.state.doc.lineAt(u.state.selection.main.head).number - 1);
      saveSession();
      // Keyboard selection or typing hides the bubble. Formatting from the bubble keeps it in place.
      if (bubble() && !u.transactions.some((tr) => tr.isUserEvent('input.format'))) setBubble(null);
      else if (bubble()) requestAnimationFrame(placeBubble);
    }
    preview?.onEditorUpdate(u);
  }

  function scheduleStats(state: EditorState) {
    clearTimeout(statsTimer);
    // Small documents update instantly. Large ones wait for a pause in typing.
    const delay = state.doc.length < 50_000 ? 0 : 250;
    statsTimer = setTimeout(() => setStats(countStats(state.doc.toString())), delay);
  }

  function updateSelectionStats(state: EditorState) {
    const ranges = state.selection.ranges.filter((r) => !r.empty);
    if (ranges.length === 0) return setSelection(null);
    const text = ranges.map((r) => state.sliceDoc(r.from, r.to)).join('\n');
    setSelection(countStats(text));
  }

  // ---------- View modes ----------
  function setMode(next: ViewMode) {
    if (plainText()) return notify('Text files show as plain text. Markdown views are for .md files.');
    const prev = mode();
    if (next === prev || !view || !preview) return;
    const sync = preview.sync;
    setBubble(null);
    // Remember the reading position from whichever pane is visible now.
    const from = showsPreview(prev) && !showsEditor(prev) ? 'preview' : 'editor';
    const line = sync.topLine(from);
    setModeSignal(next);
    update({ viewMode: next });
    saveSession();
    // Source <-> Live changes line heights a lot, so keep the same line at the top.
    applyLiveMode();
    requestAnimationFrame(() => {
      if (!view || !preview) return;
      view.requestMeasure();
      if (showsEditor(prev) && showsEditor(next) && (prev === 'live') !== (next === 'live')) scrollLineToTop(view, line);
      if (showsPreview(next)) {
        preview.invalidate();
        preview.renderNow();
      }
      if (showsEditor(next) && !showsEditor(prev)) sync.scrollToLine('editor', line);
      if (showsPreview(next) && !showsPreview(prev)) sync.scrollToLine('preview', line);
      if (next === 'split') sync.alignFrom(from);
      if (showsEditor(next)) view.focus();
    });
  }

  function cycleMode() {
    const i = VIEW_MODES.indexOf(mode());
    setMode(VIEW_MODES[(i + 1) % VIEW_MODES.length]);
  }

  // ---------- Window title ----------
  createEffect(() => {
    const d = activeDoc();
    const name = d ? docName(d) : BRAND.name;
    const pending = d && (d.status === 'dirty' || d.status === 'saving' || d.conflict) ? ' •' : '';
    void platform.setWindowTitle(`${name}${pending} — ${space()?.name ?? BRAND.name}`);
  });

  // ---------- Appearance ----------
  // Diagrams render per colour scheme. Re-render them when it changes.
  createEffect(
    on(
      effectiveScheme,
      () => {
        view?.dispatch({ effects: refreshLive.of(null) });
        if (preview && showsPreview(mode())) {
          preview.invalidate();
          preview.renderNow();
        }
      },
      { defer: true },
    ),
  );
  // The scheme in effect and its colour theme, as attributes the tokens key off (styles/tokens.css).
  createEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = effectiveScheme();
    root.dataset.colorTheme = activeColorTheme();
    if (settings.fontFamily === 'style') delete root.dataset.font;
    else root.dataset.font = settings.fontFamily;
    requestAnimationFrame(() => {
      view?.requestMeasure();
      preview?.sync.refresh();
    });
  });
  createEffect(() => {
    document.documentElement.style.setProperty('--font-scale', String(settings.fontScale));
    document.documentElement.style.setProperty('--measure-scale', String(TEXT_WIDTH_SCALE[settings.textWidth]));
    requestAnimationFrame(() => {
      view?.requestMeasure();
      preview?.sync.refresh();
    });
  });
  createEffect(on(() => settings.previewTheme, () => preview?.sync.refresh(), { defer: true }));
  createEffect(on(() => settings.scrollSync, (enabled) => preview?.sync.setEnabled(enabled)));

  // Saved under a new name (Save As to .txt, say): Markdown views on or off for this tab.
  createEffect(on(plainText, () => queueMicrotask(applyLiveMode), { defer: true }));

  // Tab switch: recount and re-render. activeId changes just before the view swaps state, so wait a tick.
  createEffect(
    on(
      () => store.activeId,
      () =>
        queueMicrotask(() => {
          if (!view) return;
          setBubble(null);
          applyLiveMode();
          scheduleStats(view.state);
          scheduleOutline(view.state, 0);
          updateSelectionStats(view.state);
          setFocusLine(view.state.doc.lineAt(view.state.selection.main.head).number - 1);
          if (preview && showsPreview(mode())) {
            preview.invalidate();
            preview.renderNow();
            // The editor restores its own scroll position on the next frame. Follow it.
            requestAnimationFrame(() => preview?.sync.alignFrom(mode() === 'preview' ? 'preview' : 'editor'));
          }
        }),
    ),
  );

  // ---------- Keyboard ----------
  function onKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return; // CodeMirror (editor-scoped commands) or a focused widget handled it
    registry.handleKey(e, isMac);
  }

  // ---------- Context menus ----------
  async function onEditorContextMenu(e: MouseEvent) {
    if (!view || e.shiftKey) return; // Shift+right-click: the native menu
    // A cell of an editable table (Live mode): rows, columns and alignment.
    const tableEl = (e.target as HTMLElement).closest('.cm-live-table') as (HTMLElement & { tableActions?: (t: Element) => TableActions | null }) | null;
    const actions = tableEl?.tableActions?.(e.target as Element);
    if (actions) {
      e.preventDefault();
      e.stopPropagation();
      const header = actions.row === 0;
      menu.openAt(e.clientX, e.clientY, [
        { label: 'Insert Row Above', disabled: header, action: () => actions.insertRow(false) },
        { label: 'Insert Row Below', action: () => actions.insertRow(true) },
        { label: 'Insert Column Left', action: () => actions.insertColumn(false) },
        { label: 'Insert Column Right', action: () => actions.insertColumn(true) },
        'separator',
        { label: 'Align Left', action: () => actions.align('left') },
        { label: 'Align Center', action: () => actions.align('center') },
        { label: 'Align Right', action: () => actions.align('right') },
        'separator',
        { label: header ? 'Delete Header Row' : 'Delete Row', danger: true, action: () => actions.deleteRow() },
        { label: 'Delete Column', danger: true, action: () => actions.deleteColumn() },
      ]);
      return;
    }
    // The nearest position, also for empty space below the text or in the margins.
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
    e.preventDefault();
    e.stopPropagation();
    const sel = view.state.selection.main;
    if (pos < sel.from || pos > sel.to) view.dispatch({ selection: { anchor: pos } });
    setBubble(null);
    const spelling = await spellingAt(pos);
    if (!view) return;
    menu.openAt(
      e.clientX,
      e.clientY,
      editorMenuItems(view, pos, {
        registry,
        platform,
        docPath: activeDoc()?.path ?? null,
        openLink: followLink,
        openPalette: (q) => setPalette(q),
        notify,
        hint,
        spellcheck: settings.spellcheck,
        spelling,
        replaceImage: (from, to) => void replaceImageAt(from, to),
        editDrawing: openDrawing,
      }),
    );
  }

  /**
   * Suggestions for the word at pos, from the OS spellchecker: [] when it's spelled right or there's
   * no word, undefined when there's no checker to ask (the menu then points to the native one).
   */
  async function spellingAt(pos: number): Promise<MenuItem[] | undefined> {
    if (!view || !settings.spellcheck) return [];
    const line = view.state.doc.lineAt(pos);
    const w = wordAt(line.text, pos - line.from);
    if (!w) return [];
    const from = line.from + w.from;
    const to = line.from + w.to;
    const lang = settings.spellLang === 'auto' ? undefined : settings.spellLang;
    const timeout = new Promise<null>((r) => setTimeout(() => r(null), 400));
    const result = await Promise.race([platform.spellCheck(w.word, lang).catch(() => null), timeout]);
    if (!result) return undefined;
    if (!result.misspelled) return [];
    return spellingItems(
      result.suggestions,
      (s) => {
        if (view?.state.sliceDoc(from, to) !== w.word) return;
        view.dispatch({ changes: { from, to, insert: s }, selection: { anchor: from + s.length }, userEvent: 'input.spell' });
        view.focus();
      },
      () =>
        void platform.spellLearn(w.word, lang).then(() => {
          view?.dispatch({ effects: recheckSpelling.of(null) });
          notify(`Added “${w.word}” to the dictionary`);
        }),
    );
  }

  function onPreviewContextMenu(e: MouseEvent) {
    if (e.shiftKey) return;
    const link = (e.target as Element).closest('a[href]')?.getAttribute('href') ?? null;
    const text = window.getSelection()?.toString() ?? '';
    const items: MenuItem[] = [];
    if (link)
      items.push(
        { label: 'Open Link', action: () => followLink(link) },
        { label: 'Copy Link Address', action: () => platform.writeClipboard(link).then(() => notify('Link copied')) },
        'separator',
      );
    items.push({ label: 'Copy', disabled: !text, action: () => platform.writeClipboard(text).then(() => notify('Copied')) });
    if (mode() === 'preview') items.push('separator', { label: 'Edit Here', hint: 'Double-click', action: () => setMode('split') });
    menu.open(e, items);
  }

  // ---------- Split divider ----------
  let workspace!: HTMLDivElement;
  function startResize(e: PointerEvent) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = workspace.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      const ratio = Math.min(0.8, Math.max(0.2, (ev.clientX - rect.left) / rect.width));
      update({ splitRatio: Math.round(ratio * 1000) / 1000 });
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      preview?.sync.refresh();
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  // ---------- Mount ----------
  let host!: HTMLDivElement;
  let previewScroller!: HTMLDivElement;
  let previewContent!: HTMLElement;

  onMount(async () => {
    const upgraded = await loadSettings().catch(() => false);
    if (platform.kind === 'tauri') {
      const current = await platform.mcpStatus().catch(() => null);
      setMcpInfo(current);
      if (settings.mcpAutostart && !current?.enabled) {
        const started = await platform.mcpStart().catch(() => null);
        if (started?.url) {
          setMcpInfo(started);
          await platform.writeClipboard(`${started.url} · token: ${started.token ?? ''}`).catch(() => undefined);
          notify(`MCP server: ${started.url} (connection copied)`);
        }
      } else if (current?.enabled) {
        // `--mcp` starts the server before the webview mounts. Make its one-time token available.
        await platform
          .writeClipboard(`${current.url} · token: ${current.token ?? ''}`)
          .catch(() => undefined);
      }
    }

    view = new EditorView({ parent: host, state: EditorState.create({ doc: '', extensions: [extensions, spellCompartment.of(spellFor())] }) });
    docs.attachView(view);
    preview = createPreview({
      view,
      platform,
      scroller: previewScroller,
      content: previewContent,
      docPath: () => activeDoc()?.path ?? null,
      isVisible: () => showsPreview(mode()),
      openPath: (p) => void docs.openPaths([p]),
      openWiki: (t) => void openWiki(t),
      colorScheme: () => effectiveScheme(),
      colorTheme: () => activeColorTheme(),
    });
    preview.sync.setEnabled(settings.scrollSync);
    if (import.meta.env.DEV) Object.assign(window, { __calmmd: { view, preview, docs, settings, explorer, sidebar, registry } });

    window.addEventListener('keydown', onKeyDown);
    // Ctrl/Cmd + mouse wheel zooms the text (and stops the webview zooming the whole page).
    let wheelAt = 0;
    const onWheel = (e: WheelEvent) => {
      if (!(isMac ? e.metaKey : e.ctrlKey)) return;
      e.preventDefault();
      const now = Date.now();
      if (now - wheelAt < 60) return; // one step per notch, even on fast trackpads
      wheelAt = now;
      registry.run(e.deltaY < 0 ? 'view.zoomIn' : 'view.zoomOut');
    };
    window.addEventListener('wheel', onWheel, { passive: false });
    // The webview's own menu (Emoji, Writing direction, More tools…) never shows: CalmMD's menus cover
    // the editor, sidebar and preview, and text fields keep theirs. Shift+right-click still opens it.
    const onNativeMenu = (e: MouseEvent) => {
      if (e.defaultPrevented || e.shiftKey) return;
      if ((e.target as HTMLElement).closest('input, textarea, [contenteditable="true"]:not(.cm-content)')) return;
      e.preventDefault();
    };
    window.addEventListener('contextmenu', onNativeMenu);
    onCleanup(() => window.removeEventListener('contextmenu', onNativeMenu));
    onCleanup(() => window.removeEventListener('wheel', onWheel));
    const onFocus = () => {
      void docs.checkExternalChanges();
      void explorer.refresh();
      void reloadThemes(); // pick up edits made to theme files in another app
      if (sidebar.open && sidebar.view === 'outline') void refreshBacklinks();
    };
    void reloadThemes();
    void platform.findPandoc().then((p) => setPandoc(!!p), () => setPandoc(false));
    window.addEventListener('focus', onFocus);
    const onEditorScroll = () => {
      saveSession();
      if (bubble()) setBubble(null);
    };
    view.scrollDOM.addEventListener('scroll', onEditorScroll, { passive: true });

    // Bubble: only for mouse selections, shown after a short pause.
    const onPointerDown = () => {
      pointerSelecting = true;
      setBubble(null);
    };
    const onPointerUp = () => {
      if (!pointerSelecting) return;
      pointerSelecting = false;
      clearTimeout(bubbleTimer);
      bubbleTimer = setTimeout(placeBubble, 150);
    };
    view.contentDOM.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('pointerup', onPointerUp);
    view.contentDOM.addEventListener('blur', () => setTimeout(() => !view?.hasFocus && setBubble(null), 100));

    // In Preview mode the outline follows the reading position.
    let previewTimer: ReturnType<typeof setTimeout> | undefined;
    const onPreviewScroll = () => {
      if (mode() !== 'preview') return;
      clearTimeout(previewTimer);
      previewTimer = setTimeout(() => preview && setFocusLine(preview.sync.topLine('preview')), 80);
    };
    previewScroller.addEventListener('scroll', onPreviewScroll, { passive: true });
    onCleanup(() => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('pointerup', onPointerUp);
      preview?.dispose();
      view?.destroy();
    });

    platform.onCloseRequested(async () => {
      await docs.takeClosingVersions();
      await Promise.all([docs.flushAll(), flushSettings(), flushSession()]);
    });
    platform.onFileDrop((paths) => {
      // Images go into the document (when one is being edited); everything else opens.
      const images = paths.filter(isImagePath);
      const rest = paths.filter((p) => !isImagePath(p));
      if (images.length && activeDoc() && showsEditor(mode())) void insertImagePaths(images);
      else rest.push(...images);
      const notes = rest.filter((p) => fileKind(p) === 'note');
      if (notes.length) void docs.openPaths(notes);
      const other = rest.find((p) => fileKind(p) !== 'note');
      if (other && !notes.length) setQuickView(other);
    });
    platform.onOpenPaths((paths) => void docs.openPaths(paths));
    platform.onBroadcast('spaces', async () => {
      await refreshSpaces();
      // This window's Space may have been renamed elsewhere.
      const s = space();
      if (s) {
        const rec = recentSpaces().find((r) => r.id === s.id);
        if (rec && rec.name !== s.name) setSpace({ id: s.id, name: rec.name });
      }
    });

    // Which session does this window load?
    //  - a window opened for a Space: that Space
    //  - the main window: the Space it had last time, else its loose session
    //  - other windows: a fresh, empty scratch window
    let session = parseSession(null);
    try {
      const wanted = platform.initialSpaceId() ?? (isMain ? (await platform.kvGet('main-space')) || null : null);
      const rec = wanted ? await spaces.get(wanted) : null;
      if (rec) {
        const holder = await platform.claimSpace(rec.id);
        if (holder) {
          void platform.focusWindow(holder);
          notify(`“${rec.name}” is already open in another window`);
        } else {
          setSpace({ id: rec.id, name: rec.name });
          session = rec.session;
        }
      } else if (isMain) {
        session = (await platform.sessionGet('session')) ?? parseSession(null);
      }
    } catch {
      // A broken session must never stop the app from starting.
    }
    if (!session.viewMode) session.viewMode = settings.viewMode;
    // Settings from before version 2: this window also moves to the new layout defaults once
    // (sidebar open, split view).
    if (upgraded) {
      session.viewMode = settings.viewMode;
      session.sidebar = { ...session.sidebar, open: true };
    }
    await explorer.loadFavorites();
    void refreshSpaces();

    try {
      await loadSession(session, isMain ? await platform.launchPaths() : []);
    } catch (e) {
      notify(`Couldn't restore your session: ${e instanceof Error ? e.message : String(e)}`);
      if (store.docs.length === 0) docs.newDoc();
    }
    sessionReady = true;
    saveSession(); // persist right away, so a crash before the first change still keeps it
    // New-version notices (desktop, first window): the one-time question, then a check about once a day.
    if (platform.kind === 'tauri' && isMain) {
      setTimeout(() => void askAboutUpdates().then(() => checkForUpdates()), 4000);
      setInterval(() => void checkForUpdates(), 6 * 60 * 60 * 1000);
    }
    if (showsPreview(mode())) preview.renderNow();
    if (showsEditor(mode())) view.focus();
    // Browser demo mode (README screenshots): ?palette=<query> opens the palette.
    // ?ui=appearance or ?ui=menu:View opens that piece of UI (screenshots of menus and popovers).
    if (platform.kind === 'web') {
      const params = new URLSearchParams(location.search);
      const q = params.get('palette');
      if (q !== null) setPalette(q);
      // ?at=<text>: the cursor goes right after that text, so Live mode shows the syntax there.
      const at = params.get('at');
      const found = at ? view.state.doc.toString().indexOf(at) : -1;
      if (found >= 0) view.dispatch({ selection: { anchor: found + at!.length }, scrollIntoView: true });
      const ui = params.get('ui');
      if (ui)
        setTimeout(() => {
          if (ui === 'appearance') document.querySelector<HTMLButtonElement>('[aria-label="Appearance"]')?.click();
          const m = /^menu:(.+)$/.exec(ui);
          if (m) {
            const [title, sub] = m[1].split('/');
            const btn = [...document.querySelectorAll<HTMLButtonElement>('.menubar-title')].find((b) => b.textContent === title);
            btn?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
            if (sub)
              setTimeout(() => {
                const item = [...document.querySelectorAll<HTMLButtonElement>('.menubar-panel .menu-item')].find((b) => b.textContent?.startsWith(sub));
                item?.click();
              }, 50);
          }
          if (ui === 'spaces') document.querySelector<HTMLButtonElement>('.space-switcher')?.click();
          if (ui === 'image')
            // Right-click the first picture once it has loaded: its Size and Align menu.
            setTimeout(() => {
              const img = document.querySelector<HTMLElement>('.pane-editor .cm-live-image img');
              const r = img?.getBoundingClientRect();
              if (img && r)
                img.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + r.width * 0.55, clientY: r.top + r.height * 0.3 }));
            }, 700);
          if (ui.startsWith('qv:')) setQuickView(ui.slice(3));
          if (ui === 'search') {
            registry.run('view.search');
            setTimeout(() => {
              const input = document.querySelector<HTMLInputElement>('.search-input');
              if (input) {
                input.value = 'chapter';
                input.dispatchEvent(new Event('input', { bubbles: true }));
              }
            }, 50);
          }
          if (ui === 'export')
            void openExport('pdf').then(() =>
              setTimeout(() => {
                const sel = document.querySelector<HTMLSelectElement>('#ex-template');
                if (sel) {
                  sel.value = 'report';
                  sel.dispatchEvent(new Event('change', { bubbles: true }));
                }
                [...document.querySelectorAll<HTMLInputElement>('.export-dialog .check-row input')][3]?.click();
              }, 100),
            );
          if (ui === 'folder') document.querySelector<HTMLButtonElement>('.folder-switcher')?.click();
        }, 300);
    }
  });

  // ---------- Sidebar resize ----------
  function startSidebarResize(e: PointerEvent) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startW = sidebar.width;
    const move = (ev: PointerEvent) => setSidebar('width', Math.round(Math.min(480, Math.max(180, startW + ev.clientX - startX))));
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      preview?.sync.refresh();
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  const revealLabel = isMac ? 'Reveal in Finder' : navigator.userAgent.includes('Windows') ? 'Show in Explorer' : 'Show in File Manager';

  return (
    <div class="app">
      <Show when={!platform.nativeMenu}>
        <MenuBar
          menus={menus}
          resolve={resolveTop}
          leading={
            <span class="menubar-logo" title={BRAND.name}>
              <LogoMark size={15} />
            </span>
          }
          trailing={<span class="menubar-title-text">{activeDoc() ? docName(activeDoc()!) : ''}</span>}
        />
      </Show>
      <div class="body-row">
        <Show when={sidebar.open}>
          <Sidebar
            view={sidebar.view}
            setView={(v) => setSidebar('view', v)}
            width={sidebar.width}
            explorer={explorer}
            activePath={activeDoc()?.path ?? null}
            openFile={(p) => openFileOrView(p)}
            quickView={(p) => setQuickView(p)}
            openMenu={menu.open}
            openMenuAt={menu.openAt}
            revealLabel={revealLabel}
            reveal={(p) => void platform.revealPath(p).catch((err) => notify(String(err)))}
            copyPath={(p) => void platform.writeClipboard(p).then(() => notify('Path copied'))}
            headings={headings()}
            activeHeading={activeHeading()}
            onHeading={(h) => gotoLine(h.line)}
            spaceName={space()?.name ?? null}
            spaceMenu={spaceMenuItems}
            backlinks={backlinks()}
            onBacklink={(b) => void openAtLine(b.path, b.line)}
            searchPanel={
              <SearchPanel
                root={explorer.state.root}
                search={(root, q, o) => platform.searchFiles(root, q, o)}
                openHit={(h) => void openAtLine(h.path, h.line)}
                openFolder={() => void explorer.openFolderDialog()}
                focusRequest={searchFocus()}
              />
            }
            historyPanel={
              <HistoryPanel
                path={activeDoc()?.path ?? null}
                revision={historyRev()}
                list={(p) => history.list(p)}
                onSave={() => void saveVersion()}
                onOpen={(v) => activeDoc()?.path && void openVersion(activeDoc()!.path!, v)}
                saveOnClose={settings.historyOnClose}
                setSaveOnClose={(on) => update({ historyOnClose: on })}
                usage={() => history.usage()}
                onClearFile={() => activeDoc()?.path && void clearFileHistory(activeDoc()!.path!)}
                onClearAll={() => void clearAllHistory()}
              />
            }
            recentSpaces={recentSpaces()}
            onOpenSpace={(id) => void switchSpace(id)}
          />
          <div class="sidebar-resizer" role="separator" aria-orientation="vertical" onPointerDown={startSidebarResize} />
        </Show>
        <div class="main-col">
          <TabStrip
            docs={docs}
            newTabHint={hint('file.new')}
            files={fileTabs()}
            activeFile={activeFile()}
            onFile={setActiveFile}
            onCloseFile={closeFileTab}
            onDoc={() => {
              setActiveFile(null);
              queueMicrotask(() => view?.focus());
            }}
          />
          <Show when={activeFileTab()} keyed>
            {(t) => (
              <div class="file-tab-view">
                <Show
                  when={fileKind(t.path) === 'drawing'}
                  fallback={
                    <QuickView
                      embedded
                      path={t.path}
                      assetUrl={(x) => platform.assetUrl(x)}
                      readText={(x) => platform.readTextFile(x).then((r) => r.text)}
                      stat={(x) => platform.stat(x)}
                      openWithDefault={(x) => void platform.openPath(x).catch((e) => notify(String(e)))}
                      reveal={(x) => void platform.revealPath(x).catch((e) => notify(String(e)))}
                      revealLabel={revealLabel}
                      onClose={() => closeFileTab(t.id)}
                    />
                  }
                >
                  <DrawingEditor
                    embedded
                    path={t.path}
                    exists={t.exists}
                    theme={effectiveScheme()}
                    readText={(p) => platform.readTextFile(p).then((r) => r.text)}
                    writeText={(p, text) => platform.writeTextFile(p, text).then(() => undefined)}
                    onSaved={(p, first) => {
                      if (first) setFileTabs(fileTabs().map((x) => (x.id === t.id ? { ...x, exists: true } : x)));
                      drawingSaved(p, first);
                    }}
                    onClose={() => closeFileTab(t.id)}
                    notify={notify}
                  />
                </Show>
              </div>
            )}
          </Show>
          <Show when={settings.toolbarVisible && showsEditor(mode()) && !plainText() && !activeFile()}>
            <Toolbar
              items={settings.toolbarItems}
              registry={registry}
              hint={hint} onCustomize={() => setCustomizing(true)}
              openMenuAt={menu.openAt}
              colors={{
                highlight: settings.highlightColor === 'default' ? null : (colorValue('highlight', settings.highlightColor) ?? null),
                text: colorValue('text', settings.textColor) ?? '#e5484d',
              }}
            />
          </Show>
          <Show when={activeDoc()?.conflict ? activeDoc() : undefined}>
            {(d) => <ConflictBanner doc={d()} docs={docs} />}
          </Show>
          <div class="workspace" data-mode={mode()} ref={workspace} style={activeFile() ? { display: 'none' } : undefined}>
            <main
              class="pane pane-editor"
              data-preview-theme={activeBase()}
              ref={host}
              onContextMenu={onEditorContextMenu}
              style={mode() === 'split' ? { flex: `0 0 ${settings.splitRatio * 100}%` } : undefined}
            />
            <div
              class="divider"
              role="separator"
              aria-orientation="vertical"
              onPointerDown={startResize}
              onDblClick={() => update({ splitRatio: 0.5 })}
            />
            <div
              class="pane pane-preview"
              ref={previewScroller}
              tabIndex={-1}
              onClick={(e) => preview?.onClick(e, { locate: mode() === 'split' })}
              onContextMenu={onPreviewContextMenu}
              onDblClick={(e) => {
                if (mode() === 'preview' && !(e.target as Element).closest('a,input')) setMode('split');
              }}
            >
              <article class="md-body" data-preview-theme={activeBase()} ref={previewContent} />
            </div>
          </div>
        </div>
      </div>
      <StatusBar
        doc={activeDoc()}
        stats={stats()}
        selection={selection()}
        message={message()}
        leading={
          <button
            class="icon-btn"
            classList={{ active: sidebar.open }}
            aria-label="Toggle sidebar"
            title={`Sidebar  ${hint('view.sidebar')}`}
            onClick={() => showSidebar()}
          >
            <IconSidebar />
          </button>
        }
        trailing={
          <>
            <Show when={mcpInfo()?.enabled}>
              <span class="mcp-indicator" title={mcpInfo()?.url ?? ''}>MCP</span>
            </Show>
            <span class="zoom-control" role="group" aria-label="Zoom">
              <button class="icon-btn" title={`Zoom out  ${hint('view.zoomOut') ?? ''}`} aria-label="Zoom out" onClick={() => registry.run('view.zoomOut')}>
                −
              </button>
              <button class="zoom-value" title={`Reset zoom  ${hint('view.zoomReset') ?? ''}`} onClick={() => registry.run('view.zoomReset')}>
                {Math.round(settings.fontScale * 100)}%
              </button>
              <button class="icon-btn" title={`Zoom in  ${hint('view.zoomIn') ?? ''}`} aria-label="Zoom in" onClick={() => registry.run('view.zoomIn')}>
                +
              </button>
            </span>
            <AppearanceMenu
              settings={settings}
              update={update}
              effectiveScheme={effectiveScheme()}
              pickColorTheme={pickColorTheme}
              userThemes={userThemes()}
              openThemesFolder={() => void openThemesFolder()}
            />
            <ViewSwitch mode={mode()} onChange={setMode} modKey={isMac ? '⌘' : 'Ctrl'} disabled={plainText()} />
          </>
        }
      />
      <Show when={bubble()}>{(b) => <SelectionBubble x={b().x} y={b().y} registry={registry} hint={hint} />}</Show>
      <Show when={palette() !== null}>
        <CommandPalette
          initial={palette()!}
          sources={paletteSources}
          onClose={() => {
            setPalette(null);
            if (showsEditor(mode())) view?.focus();
          }}
        />
      </Show>
      <Show when={customizing()}>
        <CustomizeToolbar
          items={settings.toolbarItems}
          visible={settings.toolbarVisible}
          registry={registry}
          onChange={(items, visible) => update({ toolbarItems: items, toolbarVisible: visible })}
          onClose={() => setCustomizing(false)}
        />
      </Show>
      <Show when={exportDialog()}>
        {(d) => (
          <ExportDialog
            kind={d().kind}
            prefs={d().prefs}
            engine={d().engine}
            coverDefaults={coverFromDoc(view?.state.doc.toString() ?? '', activeDoc() ? docName(activeDoc()!).replace(/\.md$/i, '') : 'Untitled', new Date())}
            themes={[...BUILTIN_THEMES.map((t) => ({ id: t.id, label: t.label })), ...userThemes().map((t) => ({ id: t.id, label: t.name }))]}
            previewThemeLabel={currentUserTheme()?.name ?? BUILTIN_THEMES.find((t) => t.id === settings.previewTheme)?.label ?? 'Paper'}
            onClose={() => setExportDialog(null)}
            onExport={(prefs, cover) => {
              const kind = d().kind;
              setExportDialog(null);
              void exporter.savePrefs(prefs);
              void (kind === 'pdf' ? exporter.exportPdf(prefs, cover) : exporter.exportHtml(prefs, cover));
            }}
          />
        )}
      </Show>
      <Show when={helpDialog() === 'shortcuts'}>
        <ShortcutsDialog registry={registry} isMac={isMac} onClose={() => setHelpDialog(null)} />
      </Show>
      <Show when={helpDialog() === 'about'}>
        <AboutDialog onClose={() => setHelpDialog(null)} />
      </Show>
      <Show when={versionView()}>
        {(vv) => (
          <VersionView
            path={vv().path}
            version={vv().version}
            versionText={vv().text}
            currentText={view?.state.doc.toString() ?? ''}
            onRestore={() => void restoreVersion()}
            onDelete={() => void deleteVersion()}
            onClose={() => setVersionView(null)}
          />
        )}
      </Show>
      <Show when={quickView()}>
        {(p) => (
          <QuickView
            path={p()}
            assetUrl={(x) => platform.assetUrl(x)}
            readText={(x) => platform.readTextFile(x).then((r) => r.text)}
            stat={(x) => platform.stat(x)}
            openWithDefault={(x) => void platform.openPath(x).catch((e) => notify(String(e)))}
            reveal={(x) => void platform.revealPath(x).catch((e) => notify(String(e)))}
            revealLabel={revealLabel}
            onClose={() => setQuickView(null)}
          />
        )}
      </Show>
      <ConfirmDialog />
      <menu.ContextMenuHost />
    </div>
  );
}
