// A small modal dialog and the form controls used inside it.

export interface DialogOptions {
  title: string;
  body: Node[];
  /** Label of the primary button; without one the dialog only has "Close". */
  submit?: string;
  /** Return a message to show it and keep the dialog open. */
  onSubmit?: () => Promise<string | void>;
  onClose?: () => void;
  wide?: boolean;
}

export interface DialogHandle {
  close: () => void;
  showError: (text: string) => void;
}

export const isDialogOpen = () => document.querySelector("dialog[open]") !== null;

export function openDialog(options: DialogOptions): DialogHandle {
  const dialog = document.createElement("dialog");
  dialog.classList.toggle("wide", options.wide ?? false);
  const form = el("form");
  const error = el("p", { className: "dialog-error", role: "alert" });
  const cancel = el("button", { type: "button", textContent: options.submit ? "Cancel" : "Close" });
  const footer = el("footer", {}, [cancel]);
  if (options.submit) {
    footer.append(el("button", { type: "submit", className: "primary", textContent: options.submit }));
  }
  form.append(
    el("h2", { textContent: options.title }),
    el("div", { className: "dialog-body" }, options.body),
    error,
    footer,
  );
  dialog.append(form);

  const close = () => dialog.close();
  cancel.onclick = close;
  dialog.addEventListener("close", () => {
    dialog.remove();
    options.onClose?.();
  });

  let submitting = false;
  form.onsubmit = async (event) => {
    event.preventDefault();
    if (submitting || !options.onSubmit) return;
    submitting = true;
    try {
      const problem = await options.onSubmit();
      if (problem) error.textContent = problem;
      else close();
    } catch (caught) {
      error.textContent = String(caught);
    } finally {
      submitting = false;
    }
  };
  // Enter makes a new line in text areas, so Mod+Enter submits from there.
  form.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && options.submit) {
      event.preventDefault();
      form.requestSubmit();
    }
  });

  document.body.append(dialog);
  dialog.showModal();
  form.querySelector<HTMLElement>(".dialog-body :is(input:not([type=checkbox]), textarea, select)")?.focus();
  return { close, showError: (text) => (error.textContent = text) };
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { role?: string } = {},
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { role, ...rest } = props;
  Object.assign(node, rest);
  if (role) node.setAttribute("role", role);
  node.append(...children);
  return node;
}

export function button(text: string, onclick: () => void, title = ""): HTMLButtonElement {
  return el("button", { type: "button", className: "outline", textContent: text, title, onclick });
}

export function input(value = "", placeholder = ""): HTMLInputElement {
  return el("input", { type: "text", value, placeholder, spellcheck: false, autocomplete: "off" });
}

export function textArea(value = "", rows = 4, code = true): HTMLTextAreaElement {
  return el("textarea", { value, rows, spellcheck: false, className: code ? "code" : "" });
}

export function field(label: string, control: Node, hint = ""): HTMLLabelElement {
  const children: (Node | string)[] = [el("span", { textContent: label }), control];
  if (hint) children.push(el("small", { textContent: hint }));
  return el("label", { className: "field" }, children);
}

export function checkbox(label: string, checked: boolean) {
  const box = el("input", { type: "checkbox", checked });
  return { box, row: el("label", { className: "check" }, [box, label]) };
}

export function select(options: [value: string, label: string][], value: string): HTMLSelectElement {
  const node = el(
    "select",
    {},
    options.map(([v, text]) => el("option", { value: v, textContent: text })),
  );
  node.value = value;
  return node;
}
