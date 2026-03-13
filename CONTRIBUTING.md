# Contributing to arx

## Development Environment

### Prerequisites

- Rust 1.75+
- Docker
- Caddy (optional)

### Setup

```bash
git clone https://github.com/arx-deploy/arx.git
cd arx
cargo build
```

### Running locally

```bash
cargo run -- server --db /tmp/arx-dev.db
```

## Pull Requests

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Run checks: `cargo clippy && cargo fmt --check && cargo test`
5. Submit a PR

### PR Checklist

- [ ] `cargo check` passes
- [ ] `cargo clippy` has no warnings
- [ ] `cargo fmt --check` passes
- [ ] New functionality has tests (if applicable)
- [ ] Commit messages follow conventions

## Code Style

- Run `cargo fmt` before committing
- No warnings from `cargo clippy`
- Error messages: lowercase, no trailing period
- Minimize `pub` visibility
- No unnecessary comments or abstractions
- `///` doc comments only for public API / clap help

## Commit Conventions

- Language: English, lowercase
- Format: `type: description`
- One logical change per commit

Types:
- `feat` — new feature
- `fix` — bug fix
- `refactor` — code restructuring
- `style` — formatting
- `docs` — documentation
- `test` — tests
- `chore` — maintenance

Examples:
```
feat: add database provisioning for mysql
fix: handle expired api keys in auth middleware
refactor: extract container cleanup into separate function
```
