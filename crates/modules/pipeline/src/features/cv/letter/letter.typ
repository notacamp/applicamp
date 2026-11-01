// Applicamp cover letter template: the "Clean" CV's page (the serif face, the
// name and contact line, the same margins) with a letter's body.
//
// The letter is DATA, never markup: everything the person wrote arrives as
// strings in `sys.inputs.cv` (JSON) and is only ever shown as text, so nothing
// they type can be read as Typst.
#let data = json(bytes(sys.inputs.cv))

#set document(
  title: if data.name != "" { data.name + " · " + data.title } else { data.title },
  author: if data.name != "" { data.name } else { "" },
)
#set page(paper: "a4", margin: (x: 2.2cm, y: 2cm))
#set text(font: "Libertinus Serif", size: 10.5pt, lang: data.lang, hyphenate: false, ligatures: false)
#set par(leading: 0.65em, justify: false)

// The name and what is under it, as on the CV.
#if data.name != "" { block(text(size: 22pt, weight: "bold")[#data.name]) }
#if data.contact.len() > 0 {
  block(above: 0.4em, text(size: 9.5pt, fill: luma(90), {
    for (i, c) in data.contact.enumerate() {
      if i > 0 { [ · ] }
      if c.url != "" { link(c.url)[#c.text] } else { [#c.text] }
    }
  }))
}
#line(length: 100%, stroke: 0.5pt + luma(190))

#v(1.2em)
#if data.greeting != "" { block(below: 1em)[#data.greeting] }
#for p in data.paragraphs {
  block(below: 0.95em)[#p]
}
#if data.closing != "" { block(above: 1.4em, below: 0.4em)[#data.closing] }
#if data.name != "" { block[#data.name] }
