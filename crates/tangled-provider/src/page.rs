//! Server-rendered hosted account pages in the Commons visual system.

use crate::FeedPost;

enum Navigation {
    Public(&'static str),
    Account(&'static str),
    Moved,
}

pub fn welcome(error: Option<&str>, posts: &[FeedPost]) -> String {
    welcome_with_values(error, posts, "", "")
}

pub fn welcome_with_values(
    error: Option<&str>,
    posts: &[FeedPost],
    name: &str,
    email: &str,
) -> String {
    layout(
        "Create an account",
        &format!(
            r#"<header class="page-heading">
<h1>Your place on Tangled.</h1>
<p class="lead">Create an account on this host to publish public posts without running your own server.</p>
</header>
<aside class="notice" aria-label="Key custody">
<strong>This host keeps your keys.</strong>
<p>It generates your identity, keeps its keys, and can sign posts as you. Choose a host you trust.</p>
</aside>
{error}
<section class="panel">
<form method="post" action="/signup">
  <label>Name <input name="name" type="text" maxlength="40" autocomplete="nickname" placeholder="What should people call you?" value="{name}" required></label>
  <label>Email <input name="email" type="email" autocomplete="email" placeholder="you@example.com" aria-describedby="email-help" value="{email}" required></label>
  <p class="field-help" id="email-help">Used to sign in to this host.</p>
  <label>Password <input name="password" type="password" minlength="8" autocomplete="new-password" aria-describedby="password-help" required></label>
  <p class="field-help" id="password-help">At least 8 characters.</p>
  <button type="submit">Create account</button>
</form>
<p class="aside">Have an account? <a href="/login">Sign in</a></p>
</section>
<p class="aside">Already hosted elsewhere? <a href="/move">Move your account here</a></p>
<p class="aside">Reed and Loomed use separate local identities. Creating a hosted account does not sign you in to those apps.</p>
<section class="timeline" aria-labelledby="public-posts"><h2 id="public-posts">Public posts on this host</h2>{posts}</section>"#,
            error = error_block(error),
            posts = post_list(posts),
            name = esc(name),
            email = esc(email),
        ),
        Navigation::Public("/"),
        None,
    )
}

pub fn login(error: Option<&str>) -> String {
    login_with_email(error, "")
}

pub fn login_with_email(error: Option<&str>, email: &str) -> String {
    layout(
        "Sign in",
        &format!(
            r#"<header class="page-heading"><h1>Welcome back.</h1>
<p class="lead">Sign in to your account on this host.</p></header>
{error}
<section class="panel">
<form method="post" action="/login">
  <label>Email <input name="email" type="email" autocomplete="email" value="{email}" required></label>
  <label>Password <input name="password" type="password" autocomplete="current-password" required></label>
  <button type="submit">Sign in</button>
</form>
<p class="aside">New here? <a href="/">Create an account</a></p>
</section>
<p class="aside">This host holds your account’s signing keys. Reed and Loomed use separate local identities.</p>"#,
            error = error_block(error),
            email = esc(email),
        ),
        Navigation::Public("/login"),
        None,
    )
}

pub fn home(name: &str, did: &str, error: Option<&str>, posts: &[FeedPost]) -> String {
    home_with_draft(name, did, error, posts, "")
}

pub fn home_with_draft(
    name: &str,
    did: &str,
    error: Option<&str>,
    posts: &[FeedPost],
    draft: &str,
) -> String {
    layout(
        "Your account",
        &format!(
            r#"<header class="page-heading"><h1>{name}</h1>
<p class="lead">Your account on this host.</p>
<p class="did">{did}</p></header>
<aside class="notice" aria-label="Key custody"><strong>This host keeps your keys.</strong>
<p>It can sign public posts as you. You can move your identity to another host.</p></aside>
{error}
<section class="panel" aria-labelledby="compose-heading">
<h2 id="compose-heading">Write a public post</h2>
<form method="post" action="/posts">
  <label>Post <textarea name="text" maxlength="2000" required placeholder="Write something public" aria-describedby="post-help">{draft}</textarea></label>
  <p class="field-help" id="post-help">Public · Up to 2,000 characters.</p>
  <button type="submit">Post</button>
</form>
</section>
<section class="timeline" aria-labelledby="public-posts"><h2 id="public-posts">Public posts on this host</h2>{posts}</section>
<p class="aside">Reed and Loomed use separate local identities. This account does not sign you in to those apps.</p>"#,
            name = esc(name),
            did = esc(did),
            error = error_block(error),
            posts = post_list(posts),
            draft = esc(draft),
        ),
        Navigation::Account("/"),
        Some((name, did)),
    )
}

pub fn move_start(error: Option<&str>) -> String {
    layout(
        "Change host",
        &format!(
            r#"<header class="page-heading"><h1>Move this account.</h1>
<p class="lead">Keep your identity. Choose a new host.</p></header>
<section class="panel">
<p>The next host will hold a new key for the same identity. When the move completes, this host deletes the old key and stops signing.</p>
<p>Create a move code when you are ready to enter it on the new host. It lasts 15 minutes.</p>
{error}
<form method="post" action="/migrate/start">
  <button type="submit">Create a move code</button>
</form>
<p class="aside"><a href="/">Back to your account</a></p>
</section>"#,
            error = error_block(error),
        ),
        Navigation::Account("/move"),
        None,
    )
}

pub fn move_stopped() -> String {
    layout(
        "Account moved",
        r#"<header class="page-heading"><h1>This host has stopped signing.</h1>
<p class="lead">Your account has moved to another host.</p></header>
<section class="panel"><h2>Your identity stays with you.</h2>
<p>This host no longer has an active signing key for the account. Sign in on the new host to continue posting.</p>
<p class="aside"><a href="/">Return to this host</a></p></section>"#,
        Navigation::Moved,
        None,
    )
}

pub fn move_code(code: &str, host: &str) -> String {
    layout(
        "Your move code",
        &format!(
            r#"<header class="page-heading"><h1>Ready for your next host.</h1>
<p class="lead">Open Move on the new host and enter these details.</p></header>
<aside class="notice" aria-label="Move code guidance"><strong>Your code lasts 15 minutes.</strong>
<p>Only enter it on the host you intend to move to. After the move, this host cannot sign for you.</p></aside>
<section class="panel">
<h2>Move code</h2>
<p class="did move-value" id="move-code">{code}</p>
<h2>Current host</h2>
<p class="did move-value" id="move-host">{host}</p>
<p class="aside">Your identity stays the same. The new host will hold its signing key.</p>
<p class="aside"><a href="/">Back to your account</a></p>
</section>"#,
            code = esc(code),
            host = esc(host),
        ),
        Navigation::Account("/move"),
        None,
    )
}

pub fn move_in(error: Option<&str>) -> String {
    layout(
        "Move here",
        &format!(
            r#"<header class="page-heading"><h1>Bring your account here.</h1>
<p class="lead">Keep your identity and choose a new home for it.</p></header>
<aside class="notice" aria-label="Key custody"><strong>This host will keep your keys.</strong>
<p>Use a move code from your current host. When the move completes, that host stops signing and this host can sign as you.</p></aside>
{error}
<section class="panel">
<form method="post" action="/move">
  <label>Current host <input name="knot" type="url" placeholder="https://old.example" required></label>
  <label>Move code <input name="code" type="text" autocomplete="one-time-code" required></label>
  <label>New password <input name="password" type="password" minlength="8" autocomplete="new-password" aria-describedby="password-help" required></label>
  <p class="field-help" id="password-help">At least 8 characters. Use this password to sign in here after the move.</p>
  <button type="submit">Move here</button>
</form>
<p class="aside"><a href="/">Create an account</a> instead.</p>
</section>"#,
            error = error_block(error),
        ),
        Navigation::Public("/move"),
        None,
    )
}

fn post_list(posts: &[FeedPost]) -> String {
    if posts.is_empty() {
        return r#"<p class="empty">No posts yet.</p>"#.into();
    }
    let mut html = String::from(r#"<ol class="posts">"#);
    for post in posts {
        html.push_str(&format!(
            r#"<li><span class="name">{name}</span><p>{text}</p></li>"#,
            name = esc(&post.name),
            text = esc(&post.text),
        ));
    }
    html.push_str("</ol>");
    html
}

fn error_block(error: Option<&str>) -> String {
    match error {
        Some(message) => format!(
            r#"<p class="error" role="alert" tabindex="-1"><strong>There was a problem.</strong> {m}</p>"#,
            m = esc(message)
        ),
        None => String::new(),
    }
}

fn layout(
    title: &str,
    body: &str,
    navigation: Navigation,
    identity: Option<(&str, &str)>,
) -> String {
    let (links, current, custody) = match navigation {
        Navigation::Public(current) => (
            vec![
                ("/", "Create account"),
                ("/login", "Sign in"),
                ("/move", "Move here"),
            ],
            current,
            "Keys held by this host",
        ),
        Navigation::Account(current) => (
            vec![
                ("/", "Your account"),
                ("/imports", "Archive & connections"),
                ("/move", "Change host"),
            ],
            current,
            "Keys held by this host",
        ),
        Navigation::Moved => (
            vec![("/", "Host home")],
            "",
            "This host has stopped signing",
        ),
    };
    let mut nav = String::new();
    for (href, label) in links {
        let selected = if href == current {
            r#" aria-current="page""#
        } else {
            ""
        };
        nav.push_str(&format!(r#"<a href="{href}"{selected}>{label}</a>"#));
    }
    let sign_out = if matches!(navigation, Navigation::Account(_)) {
        r#"<form method="post" action="/logout"><button class="ghost" type="submit">Sign out</button></form>"#
    } else {
        ""
    };
    let identity = identity
        .map(|(name, did)| {
            format!(
                r#"<strong>{name}</strong><span class="did">{did}</span>"#,
                name = esc(name),
                did = esc(did)
            )
        })
        .unwrap_or_else(|| "<strong>Hosted provider</strong>".into());
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} — Tangled</title>
  <style>{css}</style>
</head>
<body>
<a class="skip-link" href="#main-content">Skip to content</a>
<div class="account-shell">
  <aside class="rail">
    <header class="rail-heading"><a class="brand" href="/">Account</a><p>Your account and its host</p></header>
    <nav aria-label="Account">{nav}</nav>
    {sign_out}
    <div class="host-anchor">{identity}<span>{custody}</span><span>Host serving this page</span></div>
  </aside>
  <div class="stage">
    <header class="suite-bar"><strong>Tangled</strong><span class="suite-current">Account</span><span class="suite-note">Your identity, your choice of host.</span></header>
    <main id="main-content" tabindex="-1"><div class="task">{body}</div></main>
  </div>
</div>
<p id="submission-status" class="submission-status" role="status" aria-live="polite"></p>
<script>{enhancement}</script>
</body>
</html>"##,
        title = esc(title),
        css = include_str!("commons.css"),
        enhancement = r#"document.querySelector('main [role="alert"]')?.focus();
for (const form of document.querySelectorAll('form')) {
  let pending = false;
  let buttons = [];
  form.addEventListener('submit', (event) => {
    if (event.defaultPrevented) return;
    if (pending) {
      event.preventDefault();
      return;
    }
    pending = true;
    form.setAttribute('aria-busy', 'true');
    buttons = Array.from(form.querySelectorAll('button[type="submit"]'), (button) => [button, button.textContent]);
    for (const [button] of buttons) {
      button.disabled = true;
      button.textContent = 'Submitting…';
    }
    document.getElementById('submission-status').textContent = 'Submitting. Please wait.';
  });
  window.addEventListener('pageshow', () => {
    pending = false;
    form.removeAttribute('aria-busy');
    for (const [button, label] of buttons) {
      button.disabled = false;
      button.textContent = label;
    }
    document.getElementById('submission-status').textContent = '';
  });
}"#,
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_pages_have_a_skip_target_and_state_appropriate_navigation() {
        let public_pages = [
            (welcome(None, &[]), "/"),
            (login(None), "/login"),
            (move_in(None), "/move"),
        ];
        for (html, current) in public_pages {
            assert!(
                html.contains(r##"href="#main-content""##),
                "skip link missing"
            );
            assert!(
                html.contains(r#"<main id="main-content""#),
                "main target missing"
            );
            assert!(html.contains(r#"<nav aria-label="Account""#));
            assert!(html.contains(&format!(r#"href="{current}" aria-current="page""#)));
            let nav = html
                .split(r#"<nav aria-label="Account""#)
                .nth(1)
                .unwrap()
                .split("</nav>")
                .next()
                .unwrap();
            assert!(
                !nav.contains("/imports"),
                "signed-in controls on public page"
            );
            assert!(!nav.contains("/logout"), "sign out on public page");
        }
        let account = home("Name", "did:key:example", None, &[]);
        assert!(account.contains(r#"href="/imports""#));
        assert!(account.contains(r#"href="/move""#));
        assert!(account.contains(r#"<form method="post" action="/logout">"#));
        assert!(!account.contains(r#"href="/login""#));
    }

    #[test]
    fn signup_discloses_host_custody_before_the_form() {
        let html = welcome(None, &[]);
        let notice = html
            .find("This host keeps your keys.")
            .expect("custody notice");
        assert!(
            notice
                < html
                    .find(r#"<form method="post" action="/signup">"#)
                    .unwrap()
        );
        assert!(html[notice..].contains("can sign"));
        assert!(html.contains("separate local identities"));
        assert!(html.contains(r#"name="password" type="password" minlength="8""#));
    }

    #[test]
    fn rendered_forms_keep_the_real_handler_contracts() {
        for (html, action, fields) in [
            (
                welcome(None, &[]),
                "/signup",
                vec!["name", "email", "password"],
            ),
            (login(None), "/login", vec!["email", "password"]),
            (home("N", "D", None, &[]), "/posts", vec!["text"]),
            (move_start(None), "/migrate/start", vec![]),
            (move_in(None), "/move", vec!["knot", "code", "password"]),
        ] {
            assert!(html.contains(&format!(r#"<form method="post" action="{action}">"#)));
            for field in fields {
                assert!(html.contains(&format!(r#"name="{field}""#)));
            }
        }
        assert!(home("N", "D", None, &[]).contains(r#"name="text" maxlength="2000""#));
        assert!(move_code("1234", "https://source.example").contains("15 minutes"));
        assert!(!move_stopped().contains(r#"action="/posts""#));
    }

    #[test]
    fn untrusted_page_values_and_posts_are_escaped() {
        let attack = r#"<script>alert("x")</script>&"#;
        let escaped = "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&amp;";
        let posts = [FeedPost {
            name: attack.into(),
            text: attack.into(),
            ts: 0,
        }];
        for html in [
            welcome(Some(attack), &posts),
            login(Some(attack)),
            home(attack, attack, Some(attack), &posts),
            move_start(Some(attack)),
            move_in(Some(attack)),
            move_code(attack, attack),
        ] {
            assert!(!html.contains(attack));
            assert!(html.contains(escaped));
        }
        for html in [
            welcome(Some(attack), &[]),
            login(Some(attack)),
            home("N", "D", Some(attack), &[]),
            move_start(Some(attack)),
            move_in(Some(attack)),
        ] {
            assert!(html.contains(r#"role="alert""#));
        }
        assert_eq!(post_list(&posts).matches(escaped).count(), 2);
    }

    async fn response_html(response: axum::response::Response) -> String {
        use http_body_util::BodyExt;
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn failed_post_requests_preserve_the_escaped_original_draft() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;

        let directory = tempfile::tempdir().unwrap();
        let provider = crate::Provider::open(directory.path(), "http://127.0.0.1:9").unwrap();
        let token = crate::create_account(
            &provider,
            &crate::SignupForm {
                email: "draft@example.com".into(),
                name: "Draft writer".into(),
                password: "never-return-this-password".into(),
            },
        )
        .unwrap();
        let app = crate::router(provider.clone());
        for (draft, escaped_draft, expected_status) in [
            (
                format!("  </textarea><script>bad()</script>&{}\n", "a".repeat(2001)),
                format!(
                    "  &lt;/textarea&gt;&lt;script&gt;bad()&lt;/script&gt;&amp;{}\n",
                    "a".repeat(2001)
                ),
                StatusCode::OK,
            ),
            (
                "  </textarea><script>bad()</script>&\n".to_string(),
                "  &lt;/textarea&gt;&lt;script&gt;bad()&lt;/script&gt;&amp;\n".to_string(),
                StatusCode::BAD_GATEWAY,
            ),
        ] {
            if expected_status == StatusCode::BAD_GATEWAY {
                crate::lock_db(&provider)
                    .execute("UPDATE accounts SET custodial = 0", [])
                    .unwrap();
            }
            let encoded: String = draft.bytes().map(|byte| format!("%{byte:02X}")).collect();
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/posts")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .header("cookie", format!("{}={token}", crate::COOKIE))
                        .body(Body::from(format!("text={encoded}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected_status);
            let html = response_html(response).await;
            let textarea = html
                .split("<textarea ")
                .nth(1)
                .unwrap()
                .split_once('>')
                .unwrap()
                .1
                .split("</textarea>")
                .next()
                .unwrap();
            assert_eq!(
                textarea, escaped_draft,
                "a failed POST must retain the exact draft"
            );
            assert!(!html.contains("<script>bad()"));
        }
    }

    #[tokio::test]
    async fn rejected_auth_requests_retain_only_safe_profile_fields() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let directory = tempfile::tempdir().unwrap();
        let provider = crate::Provider::open(directory.path(), "http://127.0.0.1:9").unwrap();
        let app = crate::router(provider);
        for (route, body, values) in [
            (
                "/signup",
                "email=reader%2B%22%3E%3C@example.com&name=Ada%22%3E%3C&password=short",
                vec!["reader+&quot;&gt;&lt;@example.com", "Ada&quot;&gt;&lt;"],
            ),
            (
                "/login",
                "email=reader%2B%22%3E%3C@example.com&password=never-return-this-password",
                vec!["reader+&quot;&gt;&lt;@example.com"],
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(route)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(response.status().is_client_error());
            let html = response_html(response).await;
            for value in values {
                assert!(
                    html.contains(&format!("value=\"{value}\"")),
                    "{route} lost a safe profile field"
                );
            }
            assert!(!html.contains("never-return-this-password"));
            let password_input = html
                .split(r#"name="password""#)
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(
                !password_input.contains("value="),
                "passwords must never be echoed"
            );
        }
    }

    #[test]
    fn returned_error_summaries_can_receive_keyboard_focus() {
        for html in [
            login(Some("Try again")),
            welcome(Some("Try again"), &[]),
            home("N", "D", Some("Try again"), &[]),
            move_in(Some("Try again")),
            move_start(Some("Try again")),
        ] {
            assert!(html.contains(r#"role="alert" tabindex="-1""#));
        }
    }

    #[test]
    fn render_design_capture_fixtures() -> std::io::Result<()> {
        let Some(directory) = std::env::var_os("TANGLED_DESIGN_CAPTURE_DIR") else {
            return Ok(());
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        let posts = [
            FeedPost {
                name: "Avery (synthetic fixture)".into(),
                text: "A public post rendered by the real provider page. This is synthetic test content, not a live account.".into(),
                ts: 0,
            },
            FeedPost {
                name: "Rowan (synthetic fixture)".into(),
                text: "A second post to check reading measure and the shared feed dividers.".into(),
                ts: 0,
            },
        ];
        let did = format!("did:key:z{}", "6MkLongIdentity".repeat(8));
        let host = format!("https://{}.example", "long-source-host-".repeat(5));
        for (name, html) in [
            ("provider-signup", welcome(None, &posts)),
            ("provider-login", login(None)),
            (
                "provider-account",
                home("Avery (synthetic fixture)", &did, None, &posts),
            ),
            ("provider-move-start", move_start(None)),
            (
                "provider-move-code",
                move_code("synthetic-fixture-code-1234567890", &host),
            ),
            ("provider-move-in", move_in(None)),
            ("provider-move-stopped", move_stopped()),
            (
                "provider-account-error",
                home_with_draft(
                    "Avery (synthetic fixture)",
                    &did,
                    Some("Synthetic error: posting failed. Please try again."),
                    &posts,
                    "A draft kept after a failed submission.\nStill here, ready to retry.",
                ),
            ),
            (
                "provider-login-error",
                login_with_email(
                    Some("Synthetic error: email or password did not match."),
                    "avery@example.com",
                ),
            ),
            (
                "provider-signup-error",
                welcome_with_values(
                    Some("Synthetic error: this email is already in use."),
                    &posts,
                    "Avery (synthetic fixture)",
                    "avery@example.com",
                ),
            ),
            (
                "provider-move-error",
                move_in(Some(
                    "Synthetic error: this move code has expired. Get a new code from your current host.",
                )),
            ),
        ] {
            let html = html.replacen(
                "<body>",
                r#"<body><div style="padding:4px 16px;background:#d8edc8;color:#153e38;font:12px system-ui">Synthetic test fixture · real Rust page renderer · forms are not connected</div>"#,
                1,
            );
            std::fs::write(directory.join(format!("{name}.html")), html)?;
        }
        Ok(())
    }
}
