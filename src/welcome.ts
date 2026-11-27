/** Shown on first launch. */
export const WELCOME = `#set page(paper: "a4", margin: 2.5cm)
#set heading(numbering: "1.1")
#set par(justify: true)

#align(center, text(20pt, weight: "bold")[Welcome to typstView])

= Getting started
Type on the left and the preview on the right follows along. Long lines wrap in
the editor; press *Alt+Z* to turn wrapping off. Use _Export PDF_ when you are done.

- Open any \`.typ\` file with the toolbar, or drop it on the window.
- Compile errors are underlined and listed below the editor.

== Mathematics
The quadratic formula:
$ x = (-b plus.minus sqrt(b^2 - 4 a c)) / (2 a) $

== A table
#table(
  columns: (1fr, auto),
  table.header[*Item*][*Count*],
  [Apples], [3],
  [Pears], [#(2 + 3)],
)
`;
