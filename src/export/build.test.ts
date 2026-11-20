// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { buildBody, buildDocument, DEFAULT_PDF, pdfCss, type ExportOptions } from './build';

const opts: ExportOptions = { title: 'My "Notes"', theme: 'paper', userCss: null, toc: false, embedImages: true };
const assets = { baseCss: '.md-body{}', fontCss: '@font-face{}', pagedJs: 'console.log("paged")' };

describe('buildBody', () => {
  const fixture = readFileSync('crates/calmmd-core/src/core/markdown/tests/fixtures/extension-corpus.md', 'utf8');

  it('matches the TypeScript baseline snapshot', async () => {
    const body = await buildBody(fixture, '/notes/extension-corpus.md', opts, { readImage: async () => null });
    expect(body).toMatchSnapshot();
  });

  it('strips editor-only attributes and disables checkboxes', async () => {
    const body = await buildBody('# T\n\n- [ ] task', null, opts, { readImage: async () => null });
    expect(body).not.toContain('data-line-start');
    expect(body).not.toContain('data-task-line');
    expect(body).toMatch(/<input[^>]*disabled/);
  });

  it('embeds local images and leaves remote ones alone', async () => {
    const readImage = vi.fn(async (p: string) => `data:image/png;base64,${btoa(p)}`);
    const body = await buildBody('![a](img/a.png) ![b](https://e.com/b.png)', '/docs/note.md', opts, { readImage });
    expect(readImage).toHaveBeenCalledWith('/docs/img/a.png');
    expect(body).toContain('src="data:image/png;base64,');
    expect(body).toContain('src="https://e.com/b.png"');
  });

  it('keeps relative images when not embedding', async () => {
    const readImage = vi.fn(async () => 'data:x');
    const body = await buildBody('![a](img/a.png)', '/docs/note.md', { ...opts, embedImages: false }, { readImage });
    expect(readImage).not.toHaveBeenCalled();
    expect(body).toContain('src="img/a.png"');
  });

  it('builds a table of contents from h1–h3', async () => {
    const body = await buildBody('# One\n\n## Two\n\n#### Deep\n\n### Three', null, { ...opts, toc: true }, { readImage: async () => null });
    const div = document.createElement('div');
    div.innerHTML = body;
    const items = [...div.querySelectorAll('nav.toc li')].map((li) => [li.className, li.querySelector('a')!.getAttribute('href'), li.textContent]);
    expect(items).toEqual([
      ['toc-1', '#one', 'One'],
      ['toc-2', '#two', 'Two'],
      ['toc-3', '#three', 'Three'],
    ]);
    expect(div.firstElementChild!.tagName).toBe('NAV');
  });

  it('highlights code even if the grammar was never loaded before', async () => {
    const body = await buildBody('```python\ndef f(x):\n    return 1\n```', null, opts, { readImage: async () => null });
    expect(body).toMatch(/<span class="tok-keyword">def<\/span>/);
  });

  it('turns video embeds into players for HTML and links for print', async () => {
    const md = '![Talk](https://youtu.be/dQw4w9WgXcQ)';
    const html = await buildBody(md, null, opts, { readImage: async () => null });
    expect(html).toContain('<iframe class="embed-frame" src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?rel=0"');
    const print = await buildBody(md, null, { ...opts, print: true }, { readImage: async () => null });
    expect(print).toContain('class="embed-link" href="https://www.youtube.com/watch?v=dQw4w9WgXcQ"');
    expect(print).not.toContain('<iframe');
  });

  it('includes KaTeX CSS only when the document has math', async () => {
    const withMath = await buildBody('$x^2$', null, opts, { readImage: async () => null });
    const plain = await buildBody('no math', null, opts, { readImage: async () => null });
    const a = { ...assets, katexCss: '.katex{KATEX}' };
    expect(buildDocument(withMath, opts, a)).toContain('.katex{KATEX}');
    expect(buildDocument(plain, opts, a)).not.toContain('.katex{KATEX}');
  });

  it('runs the post-process hook (for math and diagrams)', async () => {
    const body = await buildBody('x', null, opts, {
      readImage: async () => null,
      postProcess: async (root) => root.insertAdjacentHTML('beforeend', '<p class="added">ok</p>'),
    });
    expect(body).toContain('class="added"');
  });

  it('uses an injected Rust renderer when provided', async () => {
    const renderMarkdown = vi.fn(async () => '<h1 id="from-rust">Rust</h1>');
    const body = await buildBody('# TS', null, { ...opts, toc: true }, { readImage: async () => null, renderMarkdown });
    expect(renderMarkdown).toHaveBeenCalledWith('# TS', null, 'paper');
    expect(body).toContain('<h1 id="from-rust">Rust</h1>');
    expect(body).toContain('href="#from-rust"');
    expect(body).not.toContain('From rust'); // heading text comes from the injected HTML
  });
});

describe('buildDocument', () => {
  it('produces a standalone page with escaped title and inlined CSS', () => {
    const html = buildDocument('<p>hi</p>', opts, assets);
    expect(html.startsWith('<!doctype html>')).toBe(true);
    expect(html).toContain('<title>My &quot;Notes&quot;</title>');
    expect(html).toContain('data-preview-theme="paper"');
    expect(html).toContain('@font-face{}');
    expect(html).not.toContain('PagedConfig'); // no Paged.js for HTML export
  });

  it('adds Paged.js and print rules for PDF', () => {
    const html = buildDocument('<p>hi</p>', opts, assets, DEFAULT_PDF);
    expect(html).toContain('PagedConfig');
    expect(html).toContain('console.log("paged")');
    expect(html).toContain('size: A4');
  });

  it('cannot be broken out of by user CSS or scripts', () => {
    const html = buildDocument('<p>x</p>', { ...opts, userCss: 'a{} </style><script>alert(1)</script>' }, { ...assets, pagedJs: 'x="</script>"' }, DEFAULT_PDF);
    expect(html).not.toContain('</style><script>alert');
    // Parsed as a browser would: only our two scripts exist, and the user CSS stays inside <style>.
    const doc = new DOMParser().parseFromString(html, 'text/html');
    expect(doc.querySelectorAll('script')).toHaveLength(2);
    // Theme CSS (with custom properties) is kept away from Paged.js, which drops them.
    expect(doc.querySelectorAll('style')).toHaveLength(2);
    expect(doc.querySelector('style[data-pagedjs-ignore]')!.textContent).toContain('a{}');
    expect(doc.querySelector('style:not([data-pagedjs-ignore])')!.textContent).toContain('@page');
    expect(doc.querySelector('script')!.textContent).toContain('PagedConfig');
  });
});

describe('pdfCss', () => {
  it('reflects page options', () => {
    const css = pdfCss(opts, { pageSize: 'Letter', margins: 'wide', header: true, pageNumbers: false, breakBeforeH1: true, template: 'standard' });
    expect(css).toContain('size: Letter');
    expect(css).toContain('30mm 28mm 32mm');
    expect(css).toContain('content: "My \\"Notes\\""');
    expect(css).not.toContain('counter(pages)');
    expect(css).toContain('h1 { break-before: page; }');
  });

  it('places the header and page numbers per template', () => {
    const base = { pageSize: 'A4', margins: 'normal', header: true, pageNumbers: true, breakBeforeH1: false } as const;
    expect(pdfCss(opts, { ...base, template: 'standard' })).toContain('@top-center');
    const report = pdfCss(opts, { ...base, template: 'report' });
    expect(report).toContain('@top-left');
    expect(report).toContain('"Page " counter(page) " of " counter(pages)');
    const minimal = pdfCss(opts, { ...base, template: 'minimal' });
    expect(minimal).not.toContain('content: "My');
    expect(minimal).toContain('@bottom-right { content: counter(page);');
  });
});
