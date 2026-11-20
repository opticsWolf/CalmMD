// @vitest-environment jsdom
/** Regenerate the 500-line source-anchor baseline with `CALMMD_UPDATE_SCROLL_ANCHORS=1 npm test`. */
import { readFileSync, writeFileSync } from 'node:fs';
import { describe, it } from 'vitest';
import { createRenderer } from './markdown';

const update = process.env.CALMMD_UPDATE_SCROLL_ANCHORS === '1';

describe('500-line source-anchor baseline', () => {
  it.skipIf(!update)('regenerates the TypeScript baseline', () => {
    const fixture = readFileSync(
      'crates/calmmd-core/src/core/markdown/tests/fixtures/scroll-sync-500.md',
      'utf8',
    );
    const html = createRenderer().render(fixture, {
      docPath: '/notes/scroll-sync-500.md',
      resolveAsset: (path) => 'asset://localhost/' + encodeURIComponent(path),
    });
    const anchors: Array<[number, number]> = [];
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
      anchors.push([startLine, Number(html.slice(end, endEnd))]);
      from = endEnd + 1;
    }
    writeFileSync(
      'src/render/__snapshots__/scroll-sync-500.anchors.json',
      JSON.stringify(anchors) + '\n',
    );
  });
});
