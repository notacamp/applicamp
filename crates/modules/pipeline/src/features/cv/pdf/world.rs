//! The only world the CV compiler sees: one embedded template, the fonts of
//! `typst-assets` compiled into the binary, and the CV as `sys.inputs`. No
//! file, no package, no network, no clock.

use std::sync::LazyLock;

use typst::{
    Library, LibraryExt, World,
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime, Dict, Duration, Str, Value},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};

/// The family both templates are set in.
const FAMILY: &str = "Libertinus Serif";

/// The fonts the templates use, parsed once.
struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .filter(|font| font.info().family == FAMILY)
        .collect();
    Fonts {
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
    }
});

/// One compilation: a template and the CV it is given.
pub struct CvWorld {
    library: LazyHash<Library>,
    main: FileId,
    source: Source,
}

impl CvWorld {
    /// The `template` (Typst source) given `json` as `sys.inputs.cv`.
    pub fn new(template: &str, json: String) -> Option<Self> {
        let mut inputs = Dict::new();
        inputs.insert(Str::from("cv"), Value::Str(Str::from(json)));
        let library = Library::builder().with_inputs(inputs).build();
        let main = RootedPath::new(VirtualRoot::Project, VirtualPath::new("cv.typ").ok()?).intern();
        Some(Self {
            library: LazyHash::new(library),
            main,
            source: Source::new(main, template.to_owned()),
        })
    }
}

impl World for CvWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Whether the embedded fonts are there (a build without them has none).
pub fn fonts_ready() -> bool {
    !FONTS.fonts.is_empty()
}
