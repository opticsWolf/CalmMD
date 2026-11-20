// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { createRenderer } from '../render/markdown';
import {
  buildAnchors,
  editorToPreview,
  previewToEditor,
  type BlockBox,
  type EditorGeometry,
} from './scrollMap';

const fixture = readFileSync(
  'crates/calmmd-core/src/core/markdown/tests/fixtures/scroll-sync-500.md',
  'utf8',
);
const baseline: Array<[number, number]> = JSON.parse(
  readFileSync('src/render/__snapshots__/scroll-sync-500.anchors.json', 'utf8'),
);
const html = createRenderer().render(fixture, {
  docPath: '/notes/scroll-sync-500.md',
  resolveAsset: (path) => 'asset://localhost/' + encodeURIComponent(path),
});

function anchorsFrom(html: string): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  let from = 0;
  for (;;) {
    const startRel = html.slice(from).indexOf('data-line-start="');
    if (startRel < 0) break;
    const start = from + startRel + 'data-line-start="'.length;
    const startEnd = html.indexOf('"', start);
    const startLine = Number(html.slice(start, startEnd));
    const endRel = html.slice(startEnd).indexOf('data-line-end="');
    if (endRel < 0) break;
    const end = startEnd + endRel + 'data-line-end="'.length;
    const endEnd = html.indexOf('"', end);
    out.push([startLine, Number(html.slice(end, endEnd))]);
    from = endEnd + 1;
  }
  return out;
}

/**
 * The browser test uses the frozen TypeScript anchor list; the Rust test in
 * `core/markdown/render.rs` asserts that Mordant emits the same list for the same fixture. Both
 * renderers therefore drive the same scroll-map maths below.
 */
describe('500-line split-view scroll sync', () => {
  it('keeps the TypeScript source anchors on the frozen baseline', () => {
    expect(anchorsFrom(html)).toEqual(baseline);
  });

  it('maps every editor top line within 0.03 lines in both directions', () => {
    const lineHeight = 20;
    const viewport = 400;
    const lineCount = fixture.split('\n').length - 1;
    expect(lineCount).toBe(500);

    const dom = document.createElement('div');
    dom.innerHTML = html;
    const tallLines = new Set(
      [...dom.querySelectorAll('[data-line-start]')]
        .filter((el) => el.querySelector('img'))
        .map((el) => Number(el.getAttribute('data-line-start'))),
    );
    const previewY: number[] = [];
    let y = 0;
    for (let line = 0; line <= lineCount; line++) {
      previewY[line] = y;
      y += lineHeight + (tallLines.has(line) ? 300 : 0);
    }

    const blocks: BlockBox[] = baseline.map(([start, end]) => ({
      start,
      end,
      top: previewY[start],
      bottom: previewY[Math.min(end, lineCount)],
    }));
    const editor: EditorGeometry = {
      lineCount,
      lineTop: (line) => line * lineHeight,
      lineBottom: (line) => (line + 1) * lineHeight,
    };
    const anchors = buildAnchors(blocks, editor, {
      aMax: lineCount * lineHeight - viewport,
      bMax: y - viewport,
    });

    let maxError = 0;
    for (let line = 0; line < lineCount; line++) {
      const editorY = editor.lineTop(line);
      if (editorY >= lineCount * lineHeight - viewport) break;
      const preview = editorToPreview(anchors, editorY);
      expect(Number.isFinite(preview)).toBe(true);
      const back = previewToEditor(anchors, preview) / lineHeight;
      maxError = Math.max(maxError, Math.abs(back - line));
      expect(Math.abs(back - line)).toBeLessThanOrEqual(0.03);
    }
    expect(maxError).toBeLessThanOrEqual(0.03);
  });
});
