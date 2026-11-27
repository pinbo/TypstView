// Autocomplete and hover help, answered by the Typst compiler.
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  snippet,
} from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import { hoverTooltip, type Tooltip } from "@codemirror/view";
import { backend } from "./backend";

/** Characters after which suggestions can appear without being asked for. */
const TRIGGER = /[\p{L}\p{N}_#@.<"(:,/-]$/u;

export function typstAssist(documentPath: () => string | null): Extension {
  async function complete(context: CompletionContext): Promise<CompletionResult | null> {
    const { state, pos, explicit } = context;
    const line = state.doc.lineAt(pos);
    if (!explicit && !TRIGGER.test(line.text.slice(0, pos - line.from))) return null;

    const found = await backend.complete(documentPath(), state.doc.toString(), pos, explicit);
    if (!found || context.aborted || found.items.length === 0) return null;
    return {
      from: found.from,
      options: found.items.map((item, i): Completion => {
        const option: Completion = {
          label: item.label,
          type: item.kind,
          detail: item.symbol ?? undefined,
          info: item.detail ?? undefined,
          // Keep Typst's own ordering among equally good matches.
          boost: -i / found.items.length,
        };
        const apply = item.apply;
        if (apply?.includes("${")) {
          // CodeMirror would also read `#{` as a placeholder.
          option.apply = snippet(apply.replace(/#\{/g, "#\\{"));
        } else if (apply) {
          option.apply = apply;
        }
        return option;
      }),
    };
  }

  const hover = hoverTooltip(async (view, pos, side): Promise<Tooltip | null> => {
    const doc = view.state.doc;
    const found = await backend.hover(documentPath(), doc.toString(), pos, side < 0);
    if (!found || view.state.doc !== doc) return null;
    return {
      pos,
      above: true,
      create: () => {
        const dom = document.createElement(found.code ? "pre" : "div");
        dom.className = "hover-help";
        dom.textContent = found.text;
        return { dom };
      },
    };
  });

  return [autocompletion({ override: [complete], icons: true }), hover];
}
