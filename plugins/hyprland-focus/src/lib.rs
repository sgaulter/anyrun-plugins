mod model;

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{HandleResult, Match, PluginInfo, get_matches, handler, info, init};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use hyprland::{
    data::Clients,
    dispatch::{Dispatch, DispatchType},
    instance::Instance,
    prelude::*,
};
use model::{Backend, Window};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    prefix: String,
    max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: ":w ".into(),
            max_entries: 10,
        }
    }
}

struct State {
    config: Config,
}

#[init]
fn init(config_dir: RString) -> State {
    let path = Path::new(config_dir.as_str()).join("hyprland-focus.ron");
    let config = match fs::read_to_string(&path) {
        Ok(text) => ron::from_str(&text).unwrap_or_else(|error| {
            eprintln!(
                "[hyprland-focus] Invalid {}: {error}; using defaults",
                path.display()
            );
            Config::default()
        }),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[hyprland-focus] Cannot read {}: {error}", path.display());
            }
            Config::default()
        }
    };
    // Read live client and group information when querying or selecting a result.
    State { config }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Hyprland windows".into(),
        icon: "preferences-system-windows".into(),
    }
}

struct HyprlandBackend;

impl Backend for HyprlandBackend {
    fn windows(&mut self) -> Result<Vec<Window>, String> {
        let instance = Instance::from_current_env().map_err(|e| e.to_string())?;
        read_windows(&instance)
    }

    fn focus(&mut self, id: u64, grouped: bool) -> Result<(), String> {
        let instance = Instance::from_current_env().map_err(|e| e.to_string())?;
        focus_window(&instance, id, grouped)
    }
}

fn read_windows(instance: &Instance) -> Result<Vec<Window>, String> {
    let clients = Clients::instance_get(instance).map_err(|e| e.to_string())?;
    Ok(clients
        .to_vec()
        .into_iter()
        .filter_map(|client| {
            // Include inactive group tabs and windows on other workspaces.
            let grouped = !client.grouped.is_empty();
            if !client.mapped || (client.hidden && !grouped) {
                return None;
            }
            let address = client.address.to_string();
            let Some(id) = model::parse_address(&address) else {
                eprintln!("[hyprland-focus] Ignoring invalid window address: {address}");
                return None;
            };
            Some(Window {
                id,
                title: client.title,
                class: client.class,
                workspace: client.workspace.name,
                focus_order: i32::from(client.focus_history_id),
                grouped,
            })
        })
        .collect())
}

fn focus_window(instance: &Instance, id: u64, grouped: bool) -> Result<(), String> {
    let expression = model::focus_expression(id);
    // The bindings' FocusWindow variant still sends the legacy focuswindow
    // command. Custom retains their IPC transport and error handling while
    // allowing Hyprland 0.56's Lua dispatcher. No shell or hyprctl subprocess.
    Dispatch::instance_call(instance, DispatchType::Custom(&expression, ""))
        .map_err(|e| e.to_string())?;
    if grouped {
        let expression = model::group_active_expression(id);
        Dispatch::instance_call(instance, DispatchType::Custom(&expression, ""))
            .map_err(|e| format!("Could not activate group member: {e}"))?;
    }
    Ok(())
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = SkimMatcherV2::default().smart_case();
    let matches = model::search(
        &mut HyprlandBackend,
        input.as_str(),
        &state.config.prefix,
        state.config.max_entries,
        |text, query| matcher.fuzzy_match(text, query),
    );
    match matches {
        Ok(windows) => windows
            .into_iter()
            .map(|window| Match {
                title: if window.title.is_empty() {
                    window.class.clone()
                } else {
                    window.title
                }
                .into(),
                description: ROption::RSome(
                    format!("{} — workspace {}", window.class, window.workspace).into(),
                ),
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RSome(window.id),
            })
            .collect(),
        Err(error) => {
            eprintln!("[hyprland-focus] Cannot list windows: {error}");
            RVec::new()
        }
    }
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    let ROption::RSome(id) = selection.id else {
        eprintln!("[hyprland-focus] Selection has no window address");
        return HandleResult::Refresh(false);
    };
    match model::activate(&mut HyprlandBackend, id) {
        Ok(()) => HandleResult::Close,
        Err(error) => {
            eprintln!("[hyprland-focus] Cannot focus window: {error}");
            HandleResult::Refresh(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::Duration,
    };

    #[test]
    fn partial_configuration_preserves_defaults() {
        let config: Config = ron::from_str("Config(prefix: \"w \")").unwrap();
        assert_eq!(config.prefix, "w ");
        assert_eq!(config.max_entries, 10);
    }

    // Exercises the real Rust binding transport without a running compositor.
    fn with_socket<T>(request: &str, response: &str, call: impl FnOnce(&Instance) -> T) -> T {
        with_exchanges(&[(request, response)], call)
    }

    fn with_exchanges<T>(exchanges: &[(&str, &str)], call: impl FnOnce(&Instance) -> T) -> T {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "anyrun-hyprland-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let listener = UnixListener::bind(directory.join(".socket.sock")).unwrap();
        let instance = Instance::from_base_socket_path(directory.clone()).unwrap();
        let exchanges: Vec<_> = exchanges
            .iter()
            .map(|(request, response)| (request.as_bytes().to_vec(), response.as_bytes().to_vec()))
            .collect();
        let server = thread::spawn(move || {
            for (request, response) in exchanges {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut received = vec![0; request.len()];
                stream.read_exact(&mut received).unwrap();
                assert_eq!(received, request);
                stream.write_all(&response).unwrap();
            }
        });
        let result = call(&instance);
        server.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
        result
    }

    #[test]
    fn rust_bindings_send_lua_focus_and_report_dispatch_errors() {
        let request = "j/dispatch hl.dsp.focus({ window = \"address:0xabcd\" }) ";
        assert!(with_socket(request, "ok", |i| focus_window(i, 0xabcd, false)).is_ok());
        let result = with_socket(request, "window not found", |i| {
            focus_window(i, 0xabcd, false)
        });
        assert!(result.unwrap_err().contains("window not found"));
    }

    #[test]
    fn grouped_window_is_focused_then_activated_by_address() {
        let focus = "j/dispatch hl.dsp.focus({ window = \"address:0xabcd\" }) ";
        let activate = "j/dispatch hl.dsp.group.active(\"address:0xabcd\") ";
        assert!(
            with_exchanges(&[(focus, "ok"), (activate, "ok")], |i| {
                focus_window(i, 0xabcd, true)
            })
            .is_ok()
        );
        let result = with_exchanges(&[(focus, "ok"), (activate, "not in group")], |i| {
            focus_window(i, 0xabcd, true)
        });
        assert!(result.unwrap_err().contains("not in group"));
        let result = with_socket(focus, "window not found", |i| focus_window(i, 0xabcd, true));
        assert!(result.unwrap_err().contains("window not found"));
    }

    #[test]
    fn hidden_group_members_are_searchable_but_other_hidden_windows_are_not() {
        let client = r#"{
            "address": "0xabcd", "mapped": true, "hidden": true, "visible": false,
            "at": [0, 0], "size": [800, 600], "workspace": {"id": 1, "name": "1"},
            "floating": false, "monitor": 0, "class": "foot", "title": "Inactive tab",
            "initialClass": "foot", "initialTitle": "Inactive tab", "pid": 1,
            "xwayland": false, "pinned": false, "fullscreen": 0, "fullscreenClient": 0,
            "grouped": ["0xabcd", "0x1234"], "swallowing": null, "focusHistoryID": 1
        }"#;
        let response = format!("[{client}]");
        let windows = with_socket("j/clients", &response, read_windows).unwrap();
        assert_eq!(windows.len(), 1);
        assert!(windows[0].grouped);
        assert_eq!(windows[0].id, 0xabcd);
        let hidden_ungrouped = response.replace(r#"["0xabcd", "0x1234"]"#, "[]");
        assert!(
            with_socket("j/clients", &hidden_ungrouped, read_windows)
                .unwrap()
                .is_empty()
        );
        let unmapped = response.replace(r#""mapped": true"#, r#""mapped": false"#);
        assert!(
            with_socket("j/clients", &unmapped, read_windows)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rust_bindings_read_live_clients_and_report_bad_json() {
        assert!(
            with_socket("j/clients", "[]", read_windows)
                .unwrap()
                .is_empty()
        );
        assert!(with_socket("j/clients", "not json", read_windows).is_err());
    }
}
