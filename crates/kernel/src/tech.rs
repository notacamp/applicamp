//! Technology names: the languages, frameworks, databases and tools a CV
//! lists. Used to tell whether a profile says anything about a stack, and
//! to find the technologies a sentence claims for a person.
//!
//! The list is facts about the world, not about any person; it errs
//! toward well-known names. A name missing from it is still checked when
//! it is written with a capital (see the sources module's grounding).

/// Lower-case names, as a token (letters, digits, `+`, `#`, `.`).
const NAMES: &[&str] = &[
    "typescript",
    "javascript",
    "ruby",
    "rails",
    "python",
    "django",
    "flask",
    "fastapi",
    "java",
    "kotlin",
    "scala",
    "swift",
    "rust",
    "golang",
    "elixir",
    "erlang",
    "haskell",
    "clojure",
    "php",
    "laravel",
    "symfony",
    "perl",
    "dart",
    "flutter",
    "c++",
    "c#",
    ".net",
    "dotnet",
    "react",
    "redux",
    "angular",
    "vue",
    "vue.js",
    "svelte",
    "next.js",
    "nextjs",
    "nuxt",
    "node",
    "node.js",
    "nodejs",
    "deno",
    "express",
    "nestjs",
    "graphql",
    "grpc",
    "html",
    "css",
    "tailwind",
    "webpack",
    "vite",
    "postgres",
    "postgresql",
    "mysql",
    "mariadb",
    "sqlite",
    "mongodb",
    "redis",
    "kafka",
    "rabbitmq",
    "elasticsearch",
    "cassandra",
    "dynamodb",
    "snowflake",
    "bigquery",
    "spark",
    "hadoop",
    "airflow",
    "dbt",
    "docker",
    "kubernetes",
    "k8s",
    "terraform",
    "ansible",
    "aws",
    "azure",
    "gcp",
    "linux",
    "nginx",
    "jenkins",
    "git",
    "android",
    "ios",
    "tensorflow",
    "pytorch",
    "pandas",
    "numpy",
    "spring",
    "hibernate",
    "sidekiq",
    "rspec",
    "jest",
    "cypress",
    "playwright",
    "selenium",
    "unity",
    "unreal",
    "solidity",
    "sql",
];

/// `token` as it is compared: lower case, edge punctuation trimmed.
fn fold(token: &str) -> String {
    token
        .trim_matches(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#' || c == '.'))
        .trim_end_matches('.')
        .to_lowercase()
}

/// Whether `token` is a known technology name.
#[must_use]
pub fn is_name(token: &str) -> bool {
    let folded = fold(token);
    !folded.is_empty() && NAMES.contains(&folded.as_str())
}

/// Whether `text` names any known technology.
#[must_use]
pub fn mentions(text: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '/' | '(' | ')' | '"' | ':'))
        .any(is_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_found_in_any_case_and_punctuation() {
        assert!(is_name("TypeScript"));
        assert!(is_name("Rails,"));
        assert!(is_name("Node.js."));
        assert!(is_name("C++"));
        assert!(!is_name("Remote"));
        assert!(!is_name(""));
    }

    #[test]
    fn text_with_a_stack_word_mentions_one() {
        assert!(mentions("Built the billing platform in Ruby/Rails"));
        assert!(!mentions("Led a team of five and ran the roadmap."));
    }
}
