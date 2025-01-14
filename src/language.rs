use std::fs::{self, File};
use std::path::{PathBuf, Path};
use std::time::SystemTime;
use std::collections::hash_map::{HashMap, Entry};
use std::sync::{Arc, Mutex};

use serde_json::from_reader;

use rocket::{Request, State};
use rocket::request::{FromRequest, Outcome};
use rocket::outcome::try_outcome;

use rocket_accept_language::AcceptLanguage;

pub type SharedLanguageCache = Arc<Mutex<LanguageCache>>;

#[inline]
pub fn new_shared_language_cache() -> SharedLanguageCache {
    Arc::new(Mutex::new(LanguageCache::default()))
}

#[derive(Debug, Clone, Default)]
pub struct LanguageCache {
    inner: HashMap<String, CachedLanguage>,
}

impl LanguageCache {
    pub fn get(&mut self, code: &str) -> Option<Language> {
        match self.inner.entry(code.to_owned()) {
            Entry::Occupied(mut oe) => {
                let file_last_modified = fs::metadata(path(code)?)
                    .map(|m| m.modified().expect("Unsupported platform"))
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                
                if oe.get().last_modified < file_last_modified {
                    if let Some(lang) = CachedLanguage::read(code) {
                        *oe.get_mut() = lang;
                        Some(oe.get().language.clone())
                    } else {
                        oe.remove();
                        None
                    }
                } else {
                    Some(oe.get().language.clone())
                }
            }
            Entry::Vacant(ve) => {
                CachedLanguage::read(code).map(|lang| {
                    ve.insert(lang).language.clone()
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
struct CachedLanguage {
    language: Language,
    last_modified: SystemTime,
}

impl CachedLanguage {
    fn read(code: &str) -> Option<Self> {
        let file = File::open(path(code)?).ok()?;
        eprintln!("Reading {}", code);
        Some(CachedLanguage {
            last_modified: file.metadata().ok()?.modified().expect("Unsupported platform"),
            language: from_reader(file).ok()?,
        })
    }
}

fn path(code: &str) -> Option<PathBuf> {
    let languages = Path::new("languages/");

    let mut path = languages.join(code);
    path.set_extension("json");
    if path.parent() != Some(languages) {
        None
    } else {
        Some(path)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LangIcon {
    code: Box<str>,
    flag: Box<str>,
    name: Box<str>
}

pub fn langs(lc: &mut LanguageCache) -> Vec<LangIcon> {
    Path::new("languages/").read_dir().unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let code = path.file_stem()?.to_os_string().into_string().ok()?.into_boxed_str();

            let lang = lc.get(&code)?;
            Some(LangIcon {
                code,
                flag: lang.flag_code,
                name: lang.display_name,
            })
        })
        .collect()
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Language {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let locales = AcceptLanguage::from_request(req).await.unwrap().accept_language;
        let slc = try_outcome!(req.guard::<&State<SharedLanguageCache>>().await).inner().clone();
        let mut lc = slc.lock().unwrap();

        let cookies = req.cookies();
        let code = if let Some(cookie) = cookies.get("lang") {
            cookie.value()
        } else {
            for locale in locales {
                let code = locale.language.as_str();

                if let Some(lang) = lc.get(code) {
                    return Outcome::Success(lang);
                }
            }
            ""
        };

        Outcome::Success(
            match lc.get(code) {
                Some(c) => c,
                None => lc.get("da").unwrap(),
            }
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Language {
    pub lang_code: Box<str>,
    pub flag_code: Box<str>,
    pub display_name: Box<str>,
    pub cookie_accept: Box<str>,
    pub index_title: Box<str>,
    pub welcome: Box<str>,
    pub new_game: Box<str>,
    pub or_join: Box<str>,
    pub game_code: Box<str>,
    pub join: Box<str>,

    pub not_found_title: Box<str>,
    pub the_page_was_not_found: Box<str>,

    pub game_title: Box<str>,
    pub write_to_opponent_here: Box<str>,

    pub game: Game,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Game {
    close_error: Box<str>,
    code: Box<str>,
    host_success: Box<str>,
    join_fail: Box<str>,
    join_success: Box<str>,
    your_turn: Box<str>,
    opponents_turn: Box<str>,
    unknown: Box<str>
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TrueFalse {
    r#true: String,
    r#false: String,
}