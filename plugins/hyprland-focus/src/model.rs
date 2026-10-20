//! Search and selection policy, independent of anyrun and compositor transport.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: u64,
    pub title: String,
    pub class: String,
    pub workspace: String,
    pub focus_order: i32,
}

pub trait Backend {
    fn windows(&mut self) -> Result<Vec<Window>, String>;
    fn focus(&mut self, id: u64) -> Result<(), String>;
}

pub fn search(
    backend: &mut impl Backend,
    input: &str,
    prefix: &str,
    limit: usize,
    score: impl Fn(&str, &str) -> Option<i64>,
) -> Result<Vec<Window>, String> {
    let Some(query) = input.strip_prefix(prefix) else {
        return Ok(Vec::new());
    };
    if limit == 0 {
        return Ok(Vec::new());
    }
    let query = query.trim();
    // Deliberately fetch on EVERY matching request, even for identical queries.
    // No client list is retained in plugin state between daemon invocations.
    let mut matches: Vec<_> = backend
        .windows()?
        .into_iter()
        .filter_map(|window| {
            let rank = if query.is_empty() {
                Some(0)
            } else {
                score(&window.title, query).max(score(&window.class, query))
            };
            rank.map(|rank| (window, rank))
        })
        .collect();
    matches.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .cmp(a_score)
            .then_with(|| focus_order(a).cmp(&focus_order(b)))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
    matches.truncate(limit);
    Ok(matches.into_iter().map(|(window, _)| window).collect())
}

fn focus_order(window: &Window) -> i32 {
    if window.focus_order < 0 {
        i32::MAX
    } else {
        window.focus_order
    }
}

pub fn activate(backend: &mut impl Backend, id: u64) -> Result<(), String> {
    // The selected window might have closed since the displayed search results.
    if !backend.windows()?.iter().any(|window| window.id == id) {
        return Err("Selected window has closed or is no longer available".into());
    }
    backend.focus(id)
}

pub fn parse_address(address: &str) -> Option<u64> {
    let digits = address.strip_prefix("0x")?;
    let id = u64::from_str_radix(digits, 16).ok()?;
    (id != 0).then_some(id)
}

pub fn focus_expression(id: u64) -> String {
    // Only a validated numeric address is interpolated, never a window title.
    format!("hl.dsp.focus({{ window = \"address:0x{id:x}\" }})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        windows: Vec<Window>,
        calls: usize,
        focused: Vec<u64>,
        fail: bool,
    }

    impl Backend for FakeBackend {
        fn windows(&mut self) -> Result<Vec<Window>, String> {
            self.calls += 1;
            if self.fail {
                Err("disconnected".into())
            } else {
                Ok(self.windows.clone())
            }
        }
        fn focus(&mut self, id: u64) -> Result<(), String> {
            self.focused.push(id);
            Ok(())
        }
    }

    fn window(id: u64, title: &str, class: &str, order: i32) -> Window {
        Window {
            id,
            title: title.into(),
            class: class.into(),
            workspace: "2: msg".into(),
            focus_order: order,
        }
    }

    fn contains(text: &str, query: &str) -> Option<i64> {
        text.contains(query).then_some(1)
    }

    #[test]
    fn repeated_query_sees_new_closed_and_renamed_windows() {
        let mut backend = FakeBackend {
            windows: vec![window(1, "old", "foot", 0)],
            ..Default::default()
        };
        let first = search(&mut backend, ":win ", ":win ", 10, contains).unwrap();
        backend.windows = vec![window(2, "new", "slack", 1)];
        let second = search(&mut backend, ":win ", ":win ", 10, contains).unwrap();
        backend.windows[0].title = "renamed".into();
        let third = search(&mut backend, ":win ", ":win ", 10, contains).unwrap();
        assert_eq!(first[0].id, 1);
        assert_eq!(second[0].id, 2);
        assert_eq!(third[0].title, "renamed");
        assert_eq!(backend.calls, 3);
    }

    #[test]
    fn disconnected_backend_does_not_return_old_results() {
        let mut backend = FakeBackend {
            windows: vec![window(1, "old", "foot", 0)],
            ..Default::default()
        };
        assert_eq!(search(&mut backend, "", "", 10, contains).unwrap().len(), 1);
        backend.fail = true;
        assert!(search(&mut backend, "", "", 10, contains).is_err());
    }

    #[test]
    fn unrelated_prefix_and_zero_limit_skip_ipc() {
        let mut backend = FakeBackend::default();
        assert!(
            search(&mut backend, "hello", ":win ", 10, contains)
                .unwrap()
                .is_empty()
        );
        assert!(
            search(&mut backend, ":win ", ":win ", 0, contains)
                .unwrap()
                .is_empty()
        );
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn prefix_is_removed_exactly_once() {
        let mut backend = FakeBackend {
            windows: vec![window(1, ":win shell", "foot", 0)],
            ..Default::default()
        };
        let matches = search(&mut backend, ":win :win shell", ":win ", 10, |_, q| {
            assert_eq!(q, ":win shell");
            Some(1)
        })
        .unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn matches_title_or_class_and_limits_results_after_ranking() {
        let mut backend = FakeBackend {
            windows: vec![window(1, "notes", "foot", 0), window(2, "team", "slack", 2)],
            ..Default::default()
        };
        let matches = search(&mut backend, "slack", "", 1, contains).unwrap();
        assert_eq!(matches[0].id, 2);
        let ranked = search(&mut backend, "query", "", 1, |text, _| {
            Some(if text == "team" { 10 } else { 1 })
        })
        .unwrap();
        assert_eq!(ranked[0].id, 2);
    }

    #[test]
    fn empty_query_prefers_recently_focused_windows() {
        let mut backend = FakeBackend {
            windows: vec![
                window(1, "unknown", "foot", -1),
                window(2, "recent", "foot", 0),
            ],
            ..Default::default()
        };
        assert_eq!(search(&mut backend, "", "", 1, contains).unwrap()[0].id, 2);
    }

    #[test]
    fn selection_uses_address_not_current_result_position() {
        let mut backend = FakeBackend {
            windows: vec![
                window(2, "second", "foot", 0),
                window(1, "first", "foot", 1),
            ],
            ..Default::default()
        };
        activate(&mut backend, 1).unwrap();
        assert_eq!(backend.focused, vec![1]);
        backend.windows.retain(|w| w.id != 1);
        assert!(activate(&mut backend, 1).is_err());
        assert_eq!(backend.focused, vec![1]);
    }

    #[test]
    fn addresses_are_validated_without_panics_or_code_injection() {
        for invalid in [
            "",
            "0x",
            "0x0",
            "abc",
            "0x10000000000000000",
            "0x1);os.exit()",
        ] {
            assert_eq!(parse_address(invalid), None);
        }
        assert_eq!(parse_address("0xAbCd"), Some(0xabcd));
        assert_eq!(
            focus_expression(0xabcd),
            "hl.dsp.focus({ window = \"address:0xabcd\" })"
        );
    }
}
