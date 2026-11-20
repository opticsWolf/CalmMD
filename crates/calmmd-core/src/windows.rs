//! Windows and Spaces. Each window holds at most one Space, and a Space is open in at most one
//! window. The map lives here (not in the frontend) so every window sees the same truth, and
//! entries disappear when their window is destroyed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[derive(Default)]
pub struct SpaceClaims {
    /// window label → space id
    by_window: Mutex<HashMap<String, String>>,
    next: AtomicU32,
}

impl SpaceClaims {
    /// Binds `space` to `window`, or reports the other window that already holds it.
    pub fn claim(&self, window: &str, space: Option<&str>) -> Option<String> {
        let mut map = self.by_window.lock().unwrap();
        if let Some(space) = space {
            if let Some((other, _)) = map.iter().find(|(w, s)| s.as_str() == space && w.as_str() != window) {
                return Some(other.clone());
            }
            map.insert(window.to_string(), space.to_string());
        } else {
            map.remove(window);
        }
        None
    }

    pub fn holder(&self, space: &str) -> Option<String> {
        let map = self.by_window.lock().unwrap();
        map.iter().find(|(_, s)| s.as_str() == space).map(|(w, _)| w.clone())
    }

    pub fn space_for_window(&self, window: &str) -> Option<String> {
        self.by_window.lock().unwrap().get(window).cloned()
    }

    pub fn release(&self, window: &str) {
        self.by_window.lock().unwrap().remove(window);
    }

    /// Allocates a stable label for a new window.
    pub fn next_label(&self) -> String {
        let n = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        format!("space-{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_space_lives_in_one_window() {
        let c = SpaceClaims::default();
        assert_eq!(c.claim("main", Some("a")), None);
        assert_eq!(c.claim("space-1", Some("a")), Some("main".to_string()));
        // Re-claiming your own space is fine.
        assert_eq!(c.claim("main", Some("a")), None);
        // Switching releases the old one.
        assert_eq!(c.claim("main", Some("b")), None);
        assert_eq!(c.claim("space-1", Some("a")), None);
        c.release("space-1");
        assert_eq!(c.claim("main", Some("a")), None);
        assert_eq!(c.claim("main", None), None);
        assert_eq!(c.claim("space-2", Some("b")), None);
        assert_eq!(c.holder("b"), Some("space-2".to_string()));
        assert_eq!(c.holder("zzz"), None);
    }
}
