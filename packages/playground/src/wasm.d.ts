declare module '../../npm/web/chordsketch_wasm.js' {
  export function render_html(input: string): string;
  export function render_text(input: string): string;
  export function render_pdf(input: string): Uint8Array;
  export function render_html_with_options(
    input: string,
    options: { transpose?: number; config?: string },
  ): string;
  export function render_text_with_options(
    input: string,
    options: { transpose?: number; config?: string },
  ): string;
  export function render_pdf_with_options(
    input: string,
    options: { transpose?: number; config?: string },
  ): Uint8Array;
  export function version(): string;
  export default function init(): Promise<void>;
}
