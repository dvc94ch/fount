use super::data::*;
use super::id::*;
use super::Registration;
use swash::{Attributes, CacheKey, FontDataRef, FontRef, Stretch, StringId};
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct ScannedFont {
    pub name: String,
    pub lowercase_name: String,
    pub index: u32,
    pub attributes: Attributes,
    pub cache_key: CacheKey,
}

#[derive(Default)]
pub struct FontScanner {
    name: String,
    font: ScannedFont,
}

impl FontScanner {
    pub fn scan(&mut self, data: &[u8], mut f: impl FnMut(&ScannedFont)) {
        if let Some(font_data) = FontDataRef::new(data) {
            let len = font_data.len();
            for i in 0..len {
                if let Some(font) = font_data.get(i) {
                    self.scan_font(&font, i as u32, &mut f);
                }
            }
        }        
    }

    fn scan_font(&mut self, font: &FontRef, index: u32, f: &mut impl FnMut(&ScannedFont)) -> Option<()> {
        self.font.name.clear();
        self.font.lowercase_name.clear();
        self.font.index = index;
        self.font.attributes = Attributes::default();
        self.name.clear();
        let strings = font.localized_strings();
        let is_var = font.variations().len() != 0;
        // Use typographic family for variable fonts that tend to encode the
        // full style in the standard family name.
        let mut name_id = if is_var {
            StringId::TypographicFamily
        } else {
            StringId::Family
        };
        if let Some(name) = strings.find_by_id(name_id, Some("en")) {
            self.font.name.extend(name.chars());
        } else if let Some(name) = strings.find_by_id(name_id, None) {
            self.font.name.extend(name.chars());
        }
        // Prefer shorter family names for the Noto fonts so that they are
        // grouped appropriately.
        if self.font.name.is_empty() || self.font.name.starts_with("Noto") {
            name_id = if name_id == StringId::Family {
                StringId::TypographicFamily
            } else {
                StringId::Family
            };
            if let Some(name) = strings.find_by_id(name_id, Some("en")) {
                self.name.extend(name.chars());
            } else if let Some(name) = strings.find_by_id(name_id, None) {
                self.name.extend(name.chars());
            }
        }
        if !self.name.is_empty() && self.name.len() < self.font.name.len() {
            core::mem::swap(&mut self.font.name, &mut self.name);
        }
        if self.font.name.is_empty() {
            if let Some(name) = strings.find_by_id(name_id, Some("en")) {
                self.font.name.extend(name.chars());
            } else if let Some(name) = strings.find_by_id(name_id, None) {
                self.font.name.extend(name.chars());
            }
        }
        if self.font.name.is_empty() {
            return None;
        }
        self.font.lowercase_name.extend(self.font.name.chars().map(|ch| ch.to_lowercase()).flatten());
        self.font.attributes = font.attributes();
        self.font.cache_key = font.key;
        f(&self.font);
        Some(())
    }
}

impl CollectionData {
    pub fn add_fonts(&mut self, scanner: &mut FontScanner, data: super::font::FontData, mut reg: Option<&mut Registration>) -> Option<u32> {
        let is_user = self.is_user;
        let source_id = SourceId::alloc(self.sources.len(), is_user)?;
        let mut added_source = false;
        let mut count = 0;
        scanner.scan(&*data, |font| {
            let font_id = if let Some(font_id) = FontId::alloc(self.fonts.len(), is_user) {
                font_id
            } else {
                return;
            };
            let family_id = if let Some(family_id) = self.family_map.get(font.lowercase_name.as_str()) {
                *family_id
            } else {
                if let Some(family_id) = FamilyId::alloc(self.families.len(), is_user) {
                    let family = FamilyData {
                        name: font.name.as_str().into(),
                        has_stretch: false,
                        fonts: Vec::new(),
                    };
                    self.families.push(Arc::new(family));
                    self.family_map.insert(font.lowercase_name.as_str().into(), family_id);
                    family_id
                } else {
                    return;
                }
            };
            let family = Arc::make_mut(self.families.get_mut(family_id.to_usize()).unwrap());
            let (stretch, weight, style) = font.attributes.parts();
            for font in &family.fonts {
                if font.1 == stretch && font.2 == weight && font.3 == style {
                    return;
                }
            }
            if !added_source {
                self.sources.push(SourceData {
                    kind: SourceDataKind::Data(data.clone()),
                    status: RwLock::new(SourceDataStatus::Vacant),
                });
                added_source = true;
            }
            if stretch != Stretch::NORMAL {
                family.has_stretch = true;
            }
            match family.fonts.binary_search_by(|probe| probe.2.cmp(&weight)) {
                Ok(index) | Err(index) => family.fonts.insert(index, (font_id, stretch, weight, style))
            }
            if let Some(reg) = reg.as_mut() {
                if !reg.families.contains(&family_id) {
                    reg.families.push(family_id);
                }
                reg.fonts.push(font_id);
            }
            self.fonts.push(FontData {
                family: family_id,
                source: source_id,
                index: font.index,
                attributes: font.attributes,
                cache_key: font.cache_key,
            });
            count += 1;
        });
        Some(count)
    }
}
