use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{HandleResult, Match, PluginInfo, get_matches, handler, info, init};
use serde::Deserialize;
use std::{fs, path::Path, process::Command};

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    command: Vec<String>,
    engines: Vec<Engine>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command: vec!["xdg-open".into()],
            engines: Vec::new(),
        }
    }
}

impl Config {
    fn opener_command(&self, destination: &str) -> Command {
        let mut command = Command::new(&self.command[0]);
        command.args(&self.command[1..]).arg(destination);
        command
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Engine {
    name: String,
    prefix: String,
    url: String,
}

impl Engine {
    fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() {
            return Err("engine name must not be empty");
        }
        Ok(())
    }

    fn is_search(&self) -> bool {
        self.url.contains("{}")
    }

    fn target_url(&self, query: &str) -> String {
        if self.is_search() {
            self.url.replace("{}", &encode_query(query))
        } else {
            self.url.clone()
        }
    }
}

// Percent-encode UTF-8 bytes, retaining only RFC 3986 unreserved characters.
fn encode_query(query: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    encoded
}

fn parse_config(text: &str) -> Result<Config, String> {
    let config: Config = ron::from_str(text).map_err(|error| error.to_string())?;
    if config
        .command
        .first()
        .is_none_or(|program| program.trim().is_empty())
    {
        return Err("command must contain an executable followed by optional arguments".into());
    }
    for engine in &config.engines {
        engine
            .validate()
            .map_err(|error| format!("{}: {error}", engine.name))?;
    }
    Ok(config)
}

#[init]
fn init(config_dir: RString) -> Config {
    let path = Path::new(config_dir.as_str()).join("multi-web.ron");
    let result = fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| parse_config(&text));
    match result {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "[multi-web] {}: {error}; no engines enabled",
                path.display()
            );
            Config::default()
        }
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Multi web".into(),
        icon: "system-search".into(),
    }
}

fn matches(input: &str, config: &Config) -> Vec<Match> {
    let Some(longest) = config
        .engines
        .iter()
        .filter(|engine| input.starts_with(&engine.prefix))
        .map(|engine| engine.prefix.len())
        .max()
    else {
        return Vec::new();
    };

    config
        .engines
        .iter()
        .enumerate()
        .filter_map(|(index, engine)| {
            if engine.prefix.len() != longest {
                return None;
            }
            let query = input.strip_prefix(&engine.prefix)?.trim();
            let is_search = engine.is_search();
            if is_search && query.is_empty() {
                return None;
            }
            Some(Match {
                title: if is_search { query } else { &engine.name }.into(),
                description: ROption::RSome(if is_search {
                    format!("Search {}", engine.name).into()
                } else {
                    format!("Open {}", engine.url).into()
                }),
                icon: ROption::RNone,
                use_pango: false,
                id: ROption::RSome(index as u64),
            })
        })
        .collect()
}

#[get_matches]
fn get_matches(input: RString, config: &Config) -> RVec<Match> {
    matches(input.as_str(), config).into()
}

fn selection_url(selection: &Match, config: &Config) -> Option<String> {
    let ROption::RSome(index) = selection.id else {
        return None;
    };
    let engine = config.engines.get(usize::try_from(index).ok()?)?;
    let query = selection.title.trim();
    (!engine.is_search() || !query.is_empty()).then(|| engine.target_url(query))
}

#[handler]
fn handler(selection: Match, config: &Config) -> HandleResult {
    let Some(url) = selection_url(&selection, config) else {
        return HandleResult::Refresh(false);
    };
    match config.opener_command(&url).spawn() {
        Ok(mut child) => {
            // Reap the opener without blocking the launcher on browser startup.
            std::thread::spawn(move || match child.wait() {
                Ok(status) if status.success() => (),
                result => eprintln!("[multi-web] Browser opener failed: {result:?}"),
            });
            HandleResult::Close
        }
        Err(error) => {
            eprintln!("[multi-web] Could not launch browser: {error}");
            HandleResult::Refresh(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        parse_config(
            r#"Config(engines: [
            (name: "Kagi", prefix: "k ", url: "https://kagi.com/search?q={}"),
            (name: "Glean", prefix: "?g", url: "https://app.glean.com/search?q={}"),
        ])"#,
        )
        .unwrap()
    }

    #[test]
    fn independent_prefixes_and_whitespace() {
        let config = config();
        let found = matches("k  rust async  ", &config);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title.as_str(), "rust async");
        assert_eq!(found[0].id, ROption::RSome(0));
        assert_eq!(matches("?gplanning", &config)[0].id, ROption::RSome(1));
        for input in ["", "k ", "?g  ", "knowledge", "other query"] {
            assert!(matches(input, &config).is_empty());
        }
    }

    #[test]
    fn longest_prefix_wins_and_shared_prefixes_offer_multiple_engines() {
        let config = parse_config(
            r#"Config(engines: [
            (name: "Default", prefix: "", url: "https://example.org/?q={}"),
            (name: "Short", prefix: "g", url: "https://example.org/?q={}"),
            (name: "Long A", prefix: "gh ", url: "https://example.org/a?q={}"),
            (name: "Long B", prefix: "gh ", url: "https://example.org/b?q={}"),
        ])"#,
        )
        .unwrap();
        let found = matches("gh issue", &config);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, ROption::RSome(2));
        assert_eq!(found[1].id, ROption::RSome(3));
        assert!(matches("gh ", &config).is_empty());
        assert_eq!(matches("ordinary query", &config)[0].id, ROption::RSome(0));
    }

    #[test]
    fn query_is_stripped_once_and_safely_encoded() {
        let config = config();
        let found = matches("k k café &x=1/#\"$(echo hi)", &config);
        assert_eq!(
            selection_url(&found[0], &config).unwrap(),
            "https://kagi.com/search?q=k%20caf%C3%A9%20%26x%3D1%2F%23%22%24%28echo%20hi%29"
        );
        assert_eq!(encode_query("a-z_A.0~+%"), "a-z_A.0~%2B%25");
    }

    #[test]
    fn invalid_configuration_is_reported() {
        for text in [
            r#"Config(command: [])"#,
            r#"Config(command: [" "])"#,
            r#"Config(engines: [(name: "", prefix: "k", url: "https://example.org/{}")])"#,
            r#"Config(engine: [])"#,
        ] {
            assert!(parse_config(text).is_err(), "{text}");
        }
        assert!(parse_config("Config()").unwrap().engines.is_empty());
    }

    #[test]
    fn default_and_custom_commands_pass_destination_as_one_literal_argument() {
        let config = parse_config("Config()").unwrap();
        let command = config.opener_command("https://example.org/");
        assert_eq!(command.get_program(), "xdg-open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["https://example.org/"]
        );

        let config = parse_config(
            r#"Config(command: ["firefox", "--new-window", "--profile", "/tmp/browser profile"])"#,
        )
        .unwrap();
        let destination = "custom:open?text=hello world&literal=$(echo test)";
        let command = config.opener_command(destination);
        assert_eq!(command.get_program(), "firefox");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--new-window",
                "--profile",
                "/tmp/browser profile",
                destination
            ]
        );
    }

    #[test]
    fn destinations_accept_any_scheme_or_no_scheme() {
        for destination in [
            "example.org/path",
            "file:///tmp/my notes",
            "custom:open?x=1&y=2",
        ] {
            let text = format!(
                "Config(engines: [(name: \"Bookmark\", prefix: \"b\", url: {destination:?})])"
            );
            let config = parse_config(&text).unwrap();
            let found = matches("b ignored text", &config);
            assert_eq!(selection_url(&found[0], &config).unwrap(), destination);
        }
        let config = parse_config(
            r#"Config(engines: [(name: "Search", prefix: "s ", url: "example.org/?q={}")])"#,
        )
        .unwrap();
        let found = matches("s a & b", &config);
        assert_eq!(
            selection_url(&found[0], &config).unwrap(),
            "example.org/?q=a%20%26%20b"
        );
    }

    #[test]
    fn bookmarks_accept_prefix_only_and_ignore_trailing_text() {
        let config = parse_config(
            r#"Config(engines: [
                (name: "Docs", prefix: "docs", url: "https://example.org/docs?a=1#intro"),
                (name: "Search docs", prefix: "docs", url: "https://example.org/search?q={}"),
            ])"#,
        )
        .unwrap();
        for input in ["docs", "docs   ", "docs anything & café"] {
            let found = matches(input, &config);
            assert_eq!(found[0].title.as_str(), "Docs");
            assert_eq!(
                selection_url(&found[0], &config).unwrap(),
                "https://example.org/docs?a=1#intro"
            );
        }
        assert_eq!(matches("docs", &config).len(), 1);
        let found = matches("docs rust book", &config);
        assert_eq!(found.len(), 2);
        assert_eq!(
            selection_url(&found[1], &config).unwrap(),
            "https://example.org/search?q=rust%20book"
        );
        assert!(matches("other", &config).is_empty());
    }

    #[test]
    fn selection_uses_engine_id_and_rejects_invalid_selections() {
        let config = config();
        let mut found = matches("?gteam", &config).remove(0);
        assert_eq!(
            selection_url(&found, &config).unwrap(),
            "https://app.glean.com/search?q=team"
        );
        found.id = ROption::RSome(u64::MAX);
        assert!(selection_url(&found, &config).is_none());
        found.id = ROption::RNone;
        assert!(selection_url(&found, &config).is_none());
        found.id = ROption::RSome(0);
        found.title = " ".into();
        assert!(selection_url(&found, &config).is_none());
    }
}
