/** DTOs returned by the Rust core Markdown service. Mirrors `crates/calmmd-core/src/core/dto.rs`. */

export interface HeadingDto {
  level: number;
  text: string;
  id: string;
  /** Zero-based source line, matching `data-line-start`. */
  line: number;
  byteStart: number;
}

export interface SourceBlockDto {
  kind: string;
  startLine: number;
  /** Inclusive, zero-based source line. */
  endLine: number;
  byteStart: number;
}

export interface MarkdownParseResult {
  headings: HeadingDto[];
  frontMatter: Record<string, unknown>;
  blocks: SourceBlockDto[];
}

export interface MarkdownRenderOptions {
  gfm?: boolean;
  footnotes?: boolean;
  attributes?: boolean;
  autoHeadingIds?: boolean;
  highlightCode?: boolean;
  /** CalmMD colour/preview theme id or a Mordant theme name. */
  theme?: string;
}

export interface MarkdownRenderResult {
  html: string;
  sourceMap: SourceBlockDto[];
}

export interface HighlightCodeResult {
  html: string;
}
