//! Editor preferences kept between sessions in `~/.config/trackstudioworld/editor.json`:
//! recent courses, favourite assets, and view settings (camera, snapping, shading, the test
//! command). Separate from `config.json`, which holds the game and mod folders.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Recent courses kept.
const RECENT: usize = 12;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    pub file: String,
    /// the mod it was opened from ("" for the game files)
    pub mod_name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Prefs {
    pub recent: Vec<Recent>,
    pub favorites: BTreeSet<String>,
    /// anything else the interface remembers, by name
    pub values: Map<String, Value>,
    path: PathBuf,
}

impl Prefs {
    pub fn file() -> PathBuf {
        crate::config::config_file().with_file_name("editor.json")
    }

    pub fn load() -> Prefs {
        Self::load_from(Self::file())
    }

    pub fn load_from(path: PathBuf) -> Prefs {
        let v: Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let recent = v["recent"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| Some(Recent { file: r["file"].as_str()?.to_string(), mod_name: r["mod"].as_str().unwrap_or("").to_string() }))
            .collect();
        let favorites = v["favorites"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect();
        let values = v["values"].as_object().cloned().unwrap_or_default();
        Prefs { recent, favorites, values, path }
    }

    pub fn save(&self) {
        let v = json!({
            "recent": self.recent.iter().map(|r| json!({"file": r.file, "mod": r.mod_name})).collect::<Vec<_>>(),
            "favorites": self.favorites,
            "values": self.values,
        });
        if let Ok(text) = serde_json::to_string_pretty(&v) {
            let _ = mkwtrack::io::write_atomic(&self.path, text.as_bytes());
        }
    }

    /// Put a course at the top of the recent list.
    pub fn opened(&mut self, file: &str, mod_name: &str) {
        let r = Recent { file: file.into(), mod_name: mod_name.into() };
        self.recent.retain(|x| x.file != r.file);
        self.recent.insert(0, r);
        self.recent.truncate(RECENT);
        self.save();
    }

    /// Star or unstar an asset; returns whether it is now a favourite.
    pub fn toggle_favorite(&mut self, name: &str) -> bool {
        let on = !self.favorites.remove(name);
        if on {
            self.favorites.insert(name.into());
        }
        self.save();
        on
    }

    pub fn set(&mut self, key: &str, v: Value) {
        self.values.insert(key.into(), v);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let f = std::env::temp_dir().join(format!("tsw-prefs-{}.json", std::process::id()));
        let mut p = Prefs::load_from(f.clone());
        for i in 0..20 {
            p.opened(&format!("Banc/{i}.bcett.byml.zs"), "");
        }
        p.opened("Banc/3.bcett.byml.zs", "MyMod");
        assert!(p.toggle_favorite("CmnObj_Coin000_A_00"));
        assert!(p.toggle_favorite("X") && !p.toggle_favorite("X"), "a second toggle unstars");
        p.set("fov", json!(70));
        let q = Prefs::load_from(f.clone());
        assert_eq!(q.recent.len(), RECENT);
        assert_eq!(q.recent[0], Recent { file: "Banc/3.bcett.byml.zs".into(), mod_name: "MyMod".into() });
        assert_eq!(q.recent.iter().filter(|r| r.file == "Banc/3.bcett.byml.zs").count(), 1);
        assert_eq!(q.favorites.iter().collect::<Vec<_>>(), ["CmnObj_Coin000_A_00"]);
        assert_eq!(q.values["fov"], json!(70));
        let _ = std::fs::remove_file(f);
    }
}
