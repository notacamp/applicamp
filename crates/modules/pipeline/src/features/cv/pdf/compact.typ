// Applicamp "Compact" CV template: the same page, tighter, for a CV that needs more lines.
//
// The CV is DATA, never markup: everything the person wrote arrives as strings
// in `sys.inputs.cv` (JSON) and is only ever shown as text, so nothing they
// type can be read as Typst. Nothing here is letter-spaced or uppercased: a
// heading must come out of the PDF's text layer as the word it is.
#let data = json(bytes(sys.inputs.cv))

#set document(
  title: if data.name != "" { data.name + " · CV" } else { "CV" },
  author: if data.name != "" { data.name } else { "" },
)
#set page(paper: "a4", margin: (x: 1.4cm, y: 1.15cm))
#set text(font: "Libertinus Serif", size: 9.5pt, lang: data.lang, hyphenate: false, ligatures: false)
#set par(leading: 0.45em, justify: false)
#set list(spacing: 0.3em, indent: 0.4em)

#show heading.where(level: 2): it => block(above: 0.8em, below: 0.35em, sticky: true)[
  #text(size: 9.5pt, weight: "bold", fill: luma(50))[#it.body]
  #v(-0.55em)
  #line(length: 100%, stroke: 0.5pt + luma(190))
]

#let section(title, body) = {
  heading(level: 2, title)
  body
}

#let when-of(x) = text(fill: luma(90))[#x]

// The name and what is under it.
#if data.name != "" { block(text(size: 17pt, weight: "bold")[#data.name]) }
#if data.headline != "" { block(above: 0.4em, text(size: 10.5pt, fill: luma(80))[#data.headline]) }
#if data.contact.len() > 0 {
  block(above: 0.4em, text(size: 9pt, fill: luma(90), {
    for (i, c) in data.contact.enumerate() {
      if i > 0 { [ · ] }
      if c.url != "" { link(c.url)[#c.text] } else { [#c.text] }
    }
  }))
}

#if data.summary != "" { section(data.labels.summary)[#data.summary] }

#if data.roles.len() > 0 {
  section(data.labels.experience, {
    for r in data.roles {
      block(above: 0.55em, below: 0.2em, breakable: false, {
        text(weight: "bold")[#r.title]
        if r.org != "" [ · #r.org]
        if r.place != "" [, #r.place]
        h(1fr)
        when-of(r.when)
      })
      if r.bullets.len() > 0 { list(..r.bullets) }
    }
  })
}

#if data.skills.len() > 0 { section(data.labels.skills)[#data.skills.join(" · ")] }

#if data.education.len() > 0 {
  section(data.labels.education, {
    for e in data.education {
      block(above: 0.35em, below: 0.1em, {
        text(weight: "bold")[#e.org]
        if e.title != "" [ · #e.title]
        h(1fr)
        when-of(e.when)
      })
    }
  })
}

#if data.languages.len() > 0 { section(data.labels.languages)[#data.languages.join(" · ")] }
