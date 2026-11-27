// The page preview. Each page is an SVG shown through an <img>, which keeps
// glyph ids of different pages apart and never runs scripts.
import type { PageOutput } from "./backend";

interface Page {
  hash: string;
  url: string;
  element: HTMLImageElement;
  /** Width in Typst points. */
  width: number;
}

const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
/** CSS pixels per Typst point at 100%. */
const PX_PER_PT = 96 / 72;

export class Preview {
  private pages: Page[] = [];
  /** `null` means fit to width. */
  private zoom: number | null = null;

  constructor(
    private readonly scroller: HTMLElement,
    private readonly container: HTMLElement,
    private readonly onZoom: (label: string) => void,
  ) {}

  get hashes(): string[] {
    return this.pages.map((p) => p.hash);
  }

  get pageCount(): number {
    return this.pages.length;
  }

  /** Update in place so the scroll position survives every keystroke. */
  render(output: PageOutput[]) {
    output.forEach((next, i) => {
      if (next.svg === null) return;
      const url = URL.createObjectURL(new Blob([next.svg], { type: "image/svg+xml" }));
      const width = Number(/<svg[^>]*?\swidth="([\d.]+)pt"/.exec(next.svg)?.[1] ?? 595);
      let page = this.pages[i];
      if (!page) {
        const element = document.createElement("img");
        element.className = "page";
        element.alt = `Page ${i + 1}`;
        element.draggable = false;
        this.container.append(element);
        page = this.pages[i] = { hash: "", url: "", element, width };
      }
      const old = page.url;
      // Free the old image only after the new one is on screen.
      page.element.onload = () => {
        if (old) URL.revokeObjectURL(old);
      };
      Object.assign(page, { hash: next.hash, url, width });
      page.element.src = url;
      this.size(page);
    });
    for (const page of this.pages.splice(output.length)) this.discard(page);
  }

  clear() {
    for (const page of this.pages.splice(0)) this.discard(page);
    this.scroller.scrollTo(0, 0);
  }

  /** Dim the pages while they lag behind a document that no longer compiles. */
  setStale(stale: boolean) {
    this.container.classList.toggle("stale", stale);
  }

  zoomBy(direction: 1 | -1) {
    const current = this.zoom ?? this.fittedZoom();
    const next =
      direction > 0
        ? ZOOM_STEPS.find((z) => z > current + 0.005)
        : [...ZOOM_STEPS].reverse().find((z) => z < current - 0.005);
    if (next !== undefined) this.setZoom(next);
  }

  fit() {
    this.setZoom(null);
  }

  private setZoom(zoom: number | null) {
    this.zoom = zoom;
    this.pages.forEach((p) => this.size(p));
    this.onZoom(zoom === null ? "Fit" : `${Math.round(zoom * 100)}%`);
  }

  private fittedZoom(): number {
    const page = this.pages[0];
    if (!page) return 1;
    return page.element.clientWidth / (page.width * PX_PER_PT) || 1;
  }

  private size(page: Page) {
    page.element.style.width =
      this.zoom === null ? "100%" : `${page.width * PX_PER_PT * this.zoom}px`;
  }

  private discard(page: Page) {
    page.element.remove();
    URL.revokeObjectURL(page.url);
  }
}
