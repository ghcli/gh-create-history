use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

/// Generates realistic file content, paths, commit messages, branch/tag names.
pub struct ContentGenerator {
    rng: Box<dyn RngCore>,
}

impl ContentGenerator {
    pub fn new(seed: Option<u64>) -> Self {
        let rng: Box<dyn RngCore> = match seed {
            Some(s) => Box::new(ChaCha8Rng::seed_from_u64(s)),
            None => Box::new(ChaCha8Rng::from_entropy()),
        };
        Self { rng }
    }

    /// Generate a blob of random file content (one of 8 content types).
    /// Size is chosen randomly in `1..=max_size`.
    pub fn generate_file_content(&mut self, max_size: u64) -> Vec<u8> {
        let size = if max_size <= 1 {
            1usize
        } else {
            self.rng.gen_range(1..=max_size) as usize
        };

        let kind = self.rng.gen_range(0u8..8);
        let raw = match kind {
            0 => self.rust_content(),
            1 => self.python_content(),
            2 => self.js_content(),
            3 => self.markdown_content(),
            4 => self.json_content(),
            5 => self.yaml_content(),
            6 => self.toml_content(),
            _ => self.plain_text_content(),
        };

        // Truncate or pad to the desired size.
        let mut buf = raw.into_bytes();
        buf.resize(size, b'\n');
        buf
    }

    /// Generate a realistic file path (1–3 directory levels).
    pub fn generate_file_path(&mut self) -> String {
        let top_dirs = [
            "src", "lib", "pkg", "internal", "cmd", "app", "core", "util", "api", "tests",
        ];
        let mid_dirs = [
            "auth", "db", "handler", "model", "service", "config", "middleware", "router",
            "schema", "adapter", "transport", "worker", "event", "cache", "metric",
        ];
        let leaf_names = [
            "handler", "main", "lib", "mod", "index", "util", "helper", "types", "error",
            "config", "schema", "factory", "manager", "controller", "provider", "client",
            "server", "registry", "context", "state",
        ];
        let extensions = ["rs", "py", "js", "ts", "go", "md", "json", "yaml", "toml", "txt"];

        let depth = self.rng.gen_range(1u8..=3);
        let top = top_dirs[self.rng.gen_range(0..top_dirs.len())];
        let leaf = leaf_names[self.rng.gen_range(0..leaf_names.len())];
        let ext = extensions[self.rng.gen_range(0..extensions.len())];

        match depth {
            1 => format!("{top}/{leaf}.{ext}"),
            2 => {
                let mid = mid_dirs[self.rng.gen_range(0..mid_dirs.len())];
                format!("{top}/{mid}/{leaf}.{ext}")
            }
            _ => {
                let mid1 = mid_dirs[self.rng.gen_range(0..mid_dirs.len())];
                let mid2 = mid_dirs[self.rng.gen_range(0..mid_dirs.len())];
                format!("{top}/{mid1}/{mid2}/{leaf}.{ext}")
            }
        }
    }

    /// Generate a conventional-commit-style message.
    pub fn generate_commit_message(&mut self) -> String {
        let templates = [
            "feat: add user authentication flow",
            "fix: correct off-by-one in pagination",
            "docs: update API reference for v2 endpoints",
            "refactor: extract validation into middleware",
            "test: add integration tests for auth module",
            "chore: bump dependency versions",
            "feat: implement webhook retry logic",
            "fix: handle null response from upstream API",
            "feat: add rate limiting to public endpoints",
            "fix: resolve race condition in cache invalidation",
            "docs: add architecture decision records",
            "refactor: simplify error handling in router",
            "test: increase coverage for edge cases",
            "chore: configure CI pipeline for nightly builds",
            "feat: support batch processing for imports",
            "fix: prevent duplicate entries on retry",
            "feat: add search functionality with filters",
            "fix: correct timezone handling in scheduler",
            "docs: improve getting-started guide",
            "refactor: migrate to async database driver",
            "feat: implement role-based access control",
            "fix: sanitize user input in query builder",
            "test: add property-based tests for serializer",
            "chore: update license headers",
            "feat: add export to CSV and JSON formats",
            "fix: handle graceful shutdown on SIGTERM",
            "feat: implement real-time notifications",
            "fix: correct memory leak in connection pool",
            "docs: document deployment procedures",
            "refactor: decouple storage from business logic",
            "feat: add health check endpoint",
            "fix: resolve deadlock in worker queue",
            "test: add load testing scenarios",
            "chore: clean up unused imports",
            "feat: implement API versioning strategy",
            "fix: correct content-type negotiation",
            "feat: add audit logging for admin actions",
            "fix: handle malformed UTF-8 in file uploads",
            "docs: add troubleshooting FAQ",
            "refactor: use builder pattern for config",
            "feat: implement caching layer with TTL",
            "fix: correct pagination cursor encoding",
            "test: add snapshot tests for templates",
            "chore: migrate to workspace dependencies",
            "feat: add multi-tenancy support",
            "fix: prevent SQL injection in dynamic queries",
            "feat: implement file upload with progress",
            "fix: correct redirect loop in auth flow",
            "docs: add runbook for incident response",
            "refactor: consolidate duplicate error types",
            "feat: add dark mode toggle to settings",
            "fix: handle network timeout in retry logic",
            "test: add fuzz tests for parser",
            "chore: set up pre-commit hooks",
            "feat: implement SSO with SAML provider",
            "fix: correct byte-range handling in downloads",
            "feat: add drag-and-drop file manager",
            "fix: resolve XSS vulnerability in comments",
            "docs: create contributor guidelines",
            "refactor: replace polling with event-driven design",
        ];
        templates[self.rng.gen_range(0..templates.len())].to_string()
    }

    /// Generate a branch name like `feature/add-auth-42`.
    pub fn generate_branch_name(&self, index: usize) -> String {
        let prefixes = ["feature", "fix", "chore", "refactor", "docs", "test"];
        let topics = [
            "add-auth", "update-deps", "fix-login", "refactor-db", "improve-perf",
            "add-search", "fix-cache", "update-ci", "add-logging", "fix-typos",
            "migrate-schema", "add-tests", "fix-cors", "add-metrics", "cleanup",
        ];
        let prefix_idx = index % prefixes.len();
        let topic_idx = index % topics.len();
        format!("{}/{}-{}", prefixes[prefix_idx], topics[topic_idx], index)
    }

    /// Generate a semver tag name derived from the index.
    pub fn generate_tag_name(&self, index: usize) -> String {
        let major = index / 100;
        let minor = (index / 10) % 10;
        let patch = index % 10;
        format!("v{major}.{minor}.{patch}")
    }

    // ---- private helpers for content types ----

    fn rust_content(&mut self) -> String {
        let names = ["Config", "Handler", "Service", "Manager", "Client", "Worker"];
        let name = names[self.rng.gen_range(0..names.len())];
        format!(
            r#"use std::collections::HashMap;

/// Auto-generated {name}.
pub struct {name} {{
    id: u64,
    data: HashMap<String, String>,
}}

impl {name} {{
    pub fn new(id: u64) -> Self {{
        Self {{ id, data: HashMap::new() }}
    }}

    pub fn id(&self) -> u64 {{
        self.id
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_new() {{
        let s = {name}::new(1);
        assert_eq!(s.id(), 1);
    }}
}}
"#
        )
    }

    fn python_content(&mut self) -> String {
        let cls = ["UserService", "DataLoader", "CacheManager", "EventBus"];
        let name = cls[self.rng.gen_range(0..cls.len())];
        format!(
            r#""""Auto-generated module."""

class {name}:
    def __init__(self, config: dict):
        self.config = config
        self._cache = {{}}

    def process(self, item: str) -> bool:
        if item in self._cache:
            return True
        self._cache[item] = True
        return False
"#
        )
    }

    fn js_content(&mut self) -> String {
        let names = ["fetchData", "processQueue", "validateInput", "transformPayload"];
        let name = names[self.rng.gen_range(0..names.len())];
        format!(
            r#"// Auto-generated module
export async function {name}(options = {{}}) {{
  const result = await fetch(options.url || '/api/data');
  if (!result.ok) throw new Error(`HTTP ${{result.status}}`);
  return result.json();
}}
"#
        )
    }

    fn markdown_content(&mut self) -> String {
        let titles = ["Architecture", "API Reference", "Deployment Guide", "Changelog"];
        let title = titles[self.rng.gen_range(0..titles.len())];
        format!(
            "# {title}\n\n## Overview\n\nThis document describes the {title} for the project.\n\n\
             ## Details\n\n- Item one\n- Item two\n- Item three\n\n## See Also\n\n- [Home](./README.md)\n"
        )
    }

    fn json_content(&mut self) -> String {
        let version = self.rng.gen_range(1u8..=5);
        format!(
            r#"{{
  "name": "generated-module",
  "version": "{version}.0.0",
  "description": "Auto-generated configuration",
  "settings": {{
    "enabled": true,
    "maxRetries": 3,
    "timeout": 30
  }}
}}
"#
        )
    }

    fn yaml_content(&mut self) -> String {
        let replicas = self.rng.gen_range(1u8..=5);
        format!(
            "---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app-config\ndata:\n  \
             replicas: \"{replicas}\"\n  logLevel: info\n  debug: \"false\"\n"
        )
    }

    fn toml_content(&mut self) -> String {
        let port = self.rng.gen_range(3000u16..9000);
        format!(
            "[server]\nhost = \"0.0.0.0\"\nport = {port}\n\n[database]\n\
             url = \"postgres://localhost/app\"\npool_size = 10\n"
        )
    }

    fn plain_text_content(&mut self) -> String {
        let id = self.rng.gen_range(1000u32..9999);
        format!(
            "Generated data file #{id}\n\nLorem ipsum dolor sit amet, \
             consectetur adipiscing elit.\nSed do eiusmod tempor incididunt \
             ut labore et dolore magna aliqua.\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_within_bounds() {
        let mut gen = ContentGenerator::new(Some(42));
        for max in [1, 10, 100, 1024, 4096] {
            let data = gen.generate_file_content(max);
            assert!(!data.is_empty(), "content must not be empty");
            assert!(
                data.len() as u64 <= max,
                "content length {} exceeds max {}",
                data.len(),
                max
            );
        }
    }

    #[test]
    fn path_has_valid_structure() {
        let mut gen = ContentGenerator::new(Some(99));
        for _ in 0..50 {
            let p = gen.generate_file_path();
            assert!(!p.is_empty());
            assert!(p.contains('/'), "path should contain a slash: {p}");
            assert!(p.contains('.'), "path should contain an extension: {p}");
            let depth = p.matches('/').count();
            assert!((1..=3).contains(&depth), "depth out of range: {p}");
        }
    }

    #[test]
    fn commit_message_not_empty() {
        let mut gen = ContentGenerator::new(Some(7));
        for _ in 0..20 {
            let msg = gen.generate_commit_message();
            assert!(!msg.is_empty());
            assert!(msg.contains(':'), "should be conventional commit: {msg}");
        }
    }

    #[test]
    fn branch_name_format() {
        let gen = ContentGenerator::new(None);
        let name = gen.generate_branch_name(42);
        assert!(name.contains('/'), "should have prefix/topic: {name}");
        assert!(name.ends_with("42"), "should end with index: {name}");
    }

    #[test]
    fn tag_name_semver() {
        let gen = ContentGenerator::new(None);
        assert_eq!(gen.generate_tag_name(0), "v0.0.0");
        assert_eq!(gen.generate_tag_name(123), "v1.2.3");
        assert_eq!(gen.generate_tag_name(10), "v0.1.0");
    }

    #[test]
    fn deterministic_with_same_seed() {
        let mut g1 = ContentGenerator::new(Some(12345));
        let mut g2 = ContentGenerator::new(Some(12345));
        for _ in 0..10 {
            assert_eq!(g1.generate_file_path(), g2.generate_file_path());
            assert_eq!(g1.generate_commit_message(), g2.generate_commit_message());
            assert_eq!(
                g1.generate_file_content(256),
                g2.generate_file_content(256)
            );
        }
    }
}
