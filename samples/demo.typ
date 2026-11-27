#set page(paper: "a5", margin: 2cm)
#include "intro.typ"

== Long line
This is one deliberately long source line so that soft wrapping in the editor is easy to check: it keeps going well past the width of the editor pane and should continue on the next visual row without any horizontal scrolling at all.

== Mathematics
$ integral_0^oo e^(-x^2) dif x = sqrt(pi) / 2 $

// A comment
#let total = (1, 2, 3).sum()
The total is *#total*.
