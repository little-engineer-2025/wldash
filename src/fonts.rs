//! Utility module for fonts

use crate::draw;
use rusttype::Font;
use std::{
    cell::RefCell, collections::HashMap, fs::File, hash, io::Read, mem, path::Path, rc::Rc, thread,
};

/// FontRef is used to store Fonts on widgets.
pub type FontRef<'a> = &'a rusttype::Font<'a>;

#[cfg(feature = "fontconfig")]
pub(crate) fn find_font(name: &str) -> String {
    use fontconfig::Fontconfig as FontConfig;
    let fc = FontConfig::new().unwrap();
    fc.find(name, None)
        .unwrap()
        .path
        .to_str()
        .unwrap()
        .to_string()
}

#[cfg(not(feature = "fontconfig"))]
pub(crate) fn find_font(_name: &str) -> String {
    panic!("fontconfig not enabled so font search not available");
}

/// FontLoader is a marker struct that is used to load files
pub(crate) struct FontLoader;

impl FontLoader {
    /// Given a path, loads it as a Font, which can be rendered to the screen.
    pub(crate) fn from_path<'a, P>(path: P) -> Option<Font<'a>>
    where
        P: AsRef<Path>,
    {
        let mut file = File::open(path).expect("Font file not found");
        let mut data = match file.metadata() {
            Ok(metadata) => Vec::with_capacity(metadata.len() as usize),
            Err(_) => vec![],
        };
        file.read_to_end(&mut data).unwrap();
        Font::try_from_vec(data)
    }
}

#[derive(Debug, Copy, Clone)]
struct ComparableF32(f32);
impl ComparableF32 {
    fn key(&self) -> u64 {
        self.0.to_bits() as u64
    }
}

impl hash::Hash for ComparableF32 {
    fn hash<H>(&self, state: &mut H)
    where
        H: hash::Hasher,
    {
        self.key().hash(state)
    }
}

impl PartialEq for ComparableF32 {
    fn eq(&self, other: &ComparableF32) -> bool {
        self.key() == other.key()
    }
}

impl Eq for ComparableF32 {}

pub struct FontMap {
    fonts: HashMap<(&'static str, ComparableF32), draw::Font<'static>>,
    font_paths: HashMap<&'static str, String>,
    required_fonts: HashMap<&'static str, (&'static str, Vec<(f32, &'static str)>)>,
}

impl FontMap {
    pub fn new() -> FontMap {
        FontMap {
            fonts: HashMap::new(),
            font_paths: HashMap::new(),
            required_fonts: HashMap::new(),
        }
    }

    pub fn queue_font(&mut self, font_name: &'static str, size: f32, preload: &'static str) {
        match self.required_fonts.get_mut(font_name) {
            Some(v) => v.1.push((size, preload)),
            None => {
                self.required_fonts
                    .insert(font_name, (font_name, vec![(size, preload)]));
            }
        }
    }

    pub fn add_font_path(&mut self, font_name: &'static str, font_path: String) {
        self.font_paths.insert(font_name, font_path);
    }

    pub fn load_fonts(&mut self) {
        for (font_name, v) in self.required_fonts.iter() {
            let path = match self.font_paths.get(font_name) {
                Some(res) => res,
                _ => {
                    let s = find_font(font_name);
                    self.font_paths.insert(font_name, s);
                    self.font_paths.get(font_name).unwrap()
                }
            };
            let fontref = Box::leak(Box::new(
                FontLoader::from_path(path).expect("unable to load font"),
            ));
            for (size, preload) in &v.1 {
                if let Some(font) = self.fonts.get_mut(&(font_name, ComparableF32(*size))) {
                    font.add_str_to_cache(preload);
                } else {
                    let mut font = draw::Font::new(fontref, *size);
                    font.add_str_to_cache(preload);
                    self.fonts.insert((font_name, ComparableF32(*size)), font);
                }
            }
        }
    }

    pub fn get_font(&mut self, font_name: &'static str, size: f32) -> &mut draw::Font<'static> {
        self.fonts
            .get_mut(&(font_name, ComparableF32(size)))
            .expect("no font at specified size")
    }
}

pub enum MaybeFontMap {
    Waiting(thread::JoinHandle<FontMap>),
    Ready(Rc<RefCell<FontMap>>),
    Invalid,
}

impl MaybeFontMap {
    pub fn unwrap(&self) -> Rc<RefCell<FontMap>> {
        match self {
            MaybeFontMap::Ready(f) => f.clone(),
            _ => panic!("fontmap not yet ready"),
        }
    }

    pub fn resolve(&mut self) {
        if matches!(self, MaybeFontMap::Waiting(_)) {
            let s = mem::replace(self, MaybeFontMap::Invalid);
            match s {
                MaybeFontMap::Waiting(handle) => {
                    *self = MaybeFontMap::Ready(Rc::new(RefCell::new(handle.join().unwrap())));
                }
                _ => unreachable!(),
            }
        }
    }
}
