// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { createRenderer, slugify } from './markdown';
import { patchChildren } from './patch';

const r = createRenderer();
const render = (s: string, docPath: string | null = null) =>
  r.render(s, { docPath, resolveAsset: (p) => 'asset://localhost/' + encodeURIComponent(p) });

function dom(html: string) {
  const div = document.createElement('div');
  div.innerHTML = html;
  return div;
}

describe('frozen extension corpus', () => {
  const fixture = readFileSync('crates/calmmd-core/src/core/markdown/tests/fixtures/extension-corpus.md', 'utf8');

  it('matches the TypeScript baseline snapshot', () => {
    expect(render(fixture, '/notes/extension-corpus.md')).toMatchSnapshot();
  });
});

describe('source line mapping', () => {
  it('annotates top-level blocks with 0-based [start, end) lines', () => {
    const d = dom(render('# Title\n\nPara one\nstill one\n\n- a\n- b\n'));
    const h1 = d.querySelector('h1')!;
    expect([h1.dataset.lineStart, h1.dataset.lineEnd]).toEqual(['0', '1']);
    const p = d.querySelector('p')!;
    expect([p.dataset.lineStart, p.dataset.lineEnd]).toEqual(['2', '4']);
    const items = [...d.querySelectorAll('li')].map((li) => li.dataset.lineStart);
    expect(items).toEqual(['5', '6']);
  });

  it('annotates code fences, tables rows, quotes and raw HTML', () => {
    const src = ['```js', 'let a = 1;', '```', '', '| a | b |', '|---|---|', '| 1 | 2 |', '', '> q', '', '<div>x</div>'].join('\n');
    const d = dom(render(src));
    expect(d.querySelector('pre')!.dataset.lineStart).toBe('0');
    expect(d.querySelector('pre')!.dataset.lineEnd).toBe('3');
    expect(d.querySelector('table')!.dataset.lineStart).toBe('4');
    const rows = [...d.querySelectorAll<HTMLElement>('tr')].map((tr) => tr.dataset.lineStart);
    expect(rows).toEqual(['4', '6']);
    expect(d.querySelector('blockquote')!.dataset.lineStart).toBe('8');
    expect(d.querySelector<HTMLElement>('.html-block')!.dataset.lineStart).toBe('10');
  });
});

describe('features', () => {
  it('renders task lists with toggle metadata', () => {
    const d = dom(render('- [ ] todo\n- [x] done\n- not a task'));
    const boxes = [...d.querySelectorAll<HTMLInputElement>('input.task-checkbox')];
    expect(boxes.map((b) => [b.dataset.taskLine, b.checked])).toEqual([
      ['0', false],
      ['1', true],
    ]);
    expect(d.querySelector('ul')!.classList.contains('contains-task-list')).toBe(true);
    expect(d.querySelectorAll('li.task-list-item')).toHaveLength(2);
    expect(d.querySelector('li.task-list-item')!.textContent!.trim()).toBe('todo');
  });

  it('gives headings unique ids', () => {
    const d = dom(render('# Hello World\n## Hello World\n## Ünïcode `code`'));
    expect([...d.querySelectorAll('h1,h2')].map((h) => h.id)).toEqual(['hello-world', 'hello-world-1', 'ünïcode-code']);
    expect(slugify('  What’s new?  ')).toBe('whats-new');
  });

  it('renders footnotes and GFM extras', () => {
    const d = dom(render('Text[^1] ~~gone~~ https://example.com\n\n[^1]: Note.'));
    expect(d.querySelector('.footnotes')).not.toBeNull();
    expect(d.querySelector('s')).not.toBeNull();
    expect(d.querySelector('a[href="https://example.com"]')).not.toBeNull();
  });

  it('resolves relative images against the document folder', () => {
    const d = dom(render('![x](img/a%20b.png) ![y](https://e.com/y.png)', '/notes/doc.md'));
    const [a, b] = [...d.querySelectorAll('img')];
    expect(a.getAttribute('src')).toBe('asset://localhost/' + encodeURIComponent('/notes/img/a b.png'));
    expect(b.getAttribute('src')).toBe('https://e.com/y.png');
  });

  it('allows data: images but not other data: links', () => {
    const d = dom(render('![s](data:image/svg+xml;utf8,%3Csvg%3E%3C/svg%3E) [x](data:text/html,hi)'));
    expect(d.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\/svg\+xml/);
    expect(d.querySelector('a')).toBeNull();
  });

  it('resolves images for Windows paths', () => {
    const d = dom(render('![x](./pic.png)', 'C:\\notes\\doc.md'));
    expect(d.querySelector('img')!.getAttribute('src')).toBe('asset://localhost/' + encodeURIComponent('C:\\notes\\pic.png'));
  });

  it('renders page-break markers with their source line', () => {
    const d = dom(render('a\n\n---pagebreak---\n\nb\n\n<!-- pagebreak -->\n\n\\newpage\n\nnot ---pagebreak--- here'));
    const breaks = [...d.querySelectorAll<HTMLElement>('.page-break')];
    expect(breaks.map((b) => b.dataset.lineStart)).toEqual(['2', '6', '8']);
    expect(d.textContent).toContain('not ---pagebreak--- here');
  });

  it('sanitises dangerous HTML', () => {
    const d = dom(render('<script>alert(1)</script><img src=x onerror="alert(1)"><a href="javascript:alert(1)">x</a><iframe src="https://e.com"></iframe>'));
    expect(d.querySelector('script')).toBeNull();
    expect(d.querySelector('iframe')).toBeNull();
    expect(d.querySelector('img')!.hasAttribute('onerror')).toBe(false);
    expect(d.querySelector('a')?.getAttribute('href') ?? null).toBeNull();
  });
});

describe('patchChildren', () => {
  it('reuses unchanged blocks and only updates line attributes', () => {
    const c = document.createElement('div');
    patchChildren(c, render('# A\n\npara\n\n- x\n- y'));
    const [h1, p, ul] = [...c.children];
    const created = patchChildren(c, render('new first\n\n# A\n\npara\n\n- x\n- y'));
    expect(created).toBe(1);
    expect(c.children[1]).toBe(h1);
    expect(c.children[2]).toBe(p);
    expect(c.children[3]).toBe(ul);
    expect((c.children[1] as HTMLElement).dataset.lineStart).toBe('2');
    expect([...c.querySelectorAll('li')].map((li) => li.dataset.lineStart)).toEqual(['6', '7']);
  });

  it('keeps a playing embed when the document re-renders', async () => {
    const { activateEmbed } = await import('./embeds');
    const { rememberOriginal } = await import('./patch');
    const c = document.createElement('div');
    patchChildren(c, render('intro\n\n![v](https://youtu.be/dQw4w9WgXcQ)'));
    const card = c.querySelector<HTMLElement>('.embed-card')!;
    rememberOriginal(card);
    activateEmbed(card);
    expect(c.querySelector('iframe')).not.toBeNull();
    patchChildren(c, render('intro, edited\n\n![v](https://youtu.be/dQw4w9WgXcQ)'));
    expect(c.querySelector('iframe')).not.toBeNull(); // still playing
    expect(c.querySelector('iframe')!.getAttribute('src')).toContain('youtube-nocookie.com/embed/dQw4w9WgXcQ');
  });

  it('produces the same DOM as a fresh render', () => {
    const c = document.createElement('div');
    patchChildren(c, render('a\n\nb\n\nc'));
    const next = render('a\n\nc\n\nb\n\nd');
    patchChildren(c, next);
    expect(c.innerHTML).toBe(dom(next).innerHTML);
  });
});
