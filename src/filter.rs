use std::path::Path;
use std::fs::Metadata;

pub trait Filter: Send + Sync {
    fn accept(&self, path: &Path, metadata: &Metadata) -> bool;
    fn describe(&self) -> String;
}

pub struct FilterChain {
    filters: Vec<Box<dyn Filter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn add(&mut self, filter: Box<dyn Filter>) {
        self.filters.push(filter);
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

impl Filter for FilterChain {
    fn accept(&self, path: &Path, metadata: &Metadata) -> bool {
        self.filters.iter().all(|f| f.accept(path, metadata))
    }

    fn describe(&self) -> String {
        if self.filters.is_empty() {
            return "no filters".to_string();
        }
        self.filters
            .iter()
            .map(|f| f.describe())
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

pub struct TypeFilter {
    extensions: Vec<String>,
    mime_globs: Vec<String>,
}

impl TypeFilter {
    pub fn new(extensions: Vec<String>, mime_globs: Vec<String>) -> Self {
        Self { extensions, mime_globs }
    }
}

impl Filter for TypeFilter {
    fn accept(&self, path: &Path, _metadata: &Metadata) -> bool {
        if self.extensions.is_empty() && self.mime_globs.is_empty() {
            return true;
        }

        let ext_match = path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| {
                let ext_lower = ext.to_lowercase();
                self.extensions.iter().any(|e| {
                    let e = e.trim_start_matches('.').to_lowercase();
                    e == ext_lower || glob::Pattern::new(&e).map_or(false, |p| p.matches(&ext_lower))
                })
            })
            .unwrap_or(false);

        let mime_match = if self.mime_globs.is_empty() {
            true
        } else {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            self.mime_globs.iter().any(|g| {
                glob::Pattern::new(g).map_or(false, |p| p.matches(&mime))
            })
        };

        ext_match || mime_match
    }

    fn describe(&self) -> String {
        format!("type({:?}, {:?})", self.extensions, self.mime_globs)
    }
}

pub struct SizeFilter {
    min: Option<u64>,
    max: Option<u64>,
}

impl SizeFilter {
    pub fn new(min: Option<u64>, max: Option<u64>) -> Self {
        Self { min, max }
    }
}

impl Filter for SizeFilter {
    fn accept(&self, _path: &Path, metadata: &Metadata) -> bool {
        let len = metadata.len();
        if let Some(min) = self.min {
            if len < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if len > max {
                return false;
            }
        }
        true
    }

    fn describe(&self) -> String {
        match (self.min, self.max) {
            (Some(min), Some(max)) => format!("size({min}..{max})"),
            (Some(min), None) => format!("size(>={min})"),
            (None, Some(max)) => format!("size(<={max})"),
            (None, None) => "size(any)".to_string(),
        }
    }
}

pub struct PatternFilter {
    patterns: Vec<glob::Pattern>,
}

impl PatternFilter {
    pub fn new(patterns: Vec<String>) -> Self {
        let compiled = patterns
            .into_iter()
            .filter_map(|p| glob::Pattern::new(&p).ok())
            .collect();
        Self { patterns: compiled }
    }
}

impl Filter for PatternFilter {
    fn accept(&self, path: &Path, _metadata: &Metadata) -> bool {
        if self.patterns.is_empty() {
            return true;
        }
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        self.patterns.iter().any(|p| p.matches(name))
    }

    fn describe(&self) -> String {
        format!("pattern({:?})", self.patterns.len())
    }
}

pub fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim().to_lowercase();
    let (num_str, multiplier) = if let Some(s) = s.strip_suffix("kb") {
        (s, 1024u64)
    } else if let Some(s) = s.strip_suffix("mb") {
        (s, 1024 * 1024)
    } else if let Some(s) = s.strip_suffix("gb") {
        (s, 1024 * 1024 * 1024)
    } else if let Some(s) = s.strip_suffix("tb") {
        (s, 1024u64.pow(4))
    } else {
        (s.as_str(), 1)
    };
    let num: u64 = num_str.trim().parse().map_err(|e| format!("invalid size: {e}"))?;
    Ok(num * multiplier)
}
