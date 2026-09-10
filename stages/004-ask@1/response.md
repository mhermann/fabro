Answers received. Restating for the planning stage:

**1. Deployment scope → Client-only.**
Build: a third `SearchBackend::SearXng` variant in `lib/components/fabro-agent/src/web_search.rs` plus its plumbing. No compose sidecar, no container management, no default URL — the operator runs their own SearXNG instance and points Fabro at it. Docs must call out that `format=json` must be enabled on the instance (off by default).

**2. Local meaning → Cost only, SearXNG.**
Build: a small GET+JSON SearXNG client using existing `fabro-http` dependencies — no new crates, no YaCy/local-index work. Queries still go to upstream engines from the SearXNG instance; docs should say so plainly so nobody mistakes this for an offline search.

**3. Routing policy → Paid-first (Brave > Venice > local).**
Build: precedence in `SearchBackend::from_secrets` becomes Brave key > Venice key > SearXNG URL; the local backend is used only when neither paid credential is configured. No failover — the documented "a failed call returns an error, no retry through the other backend" contract stays exactly as is. This means zero behavior change for existing installs; the new backend only activates where no paid key exists.

Combined with the decisions already settled (vault-stored `SEARXNG_URL` as the config surface per the `88ed2ac9` credential-presence precedent, optional `SEARXNG_API_KEY` header, ~30s timeout, client-side `max_results` truncation, shared output format via `format_hits`, doctor probe with a "JSON likely disabled" remediation on 403, full docs/changelog/test coverage per the Venice diff), the change is now fully determined.