//! Server-rendered account pages. One column, the site's slate and copper.

use crate::FeedPost;

pub fn welcome(error: Option<&str>, posts: &[FeedPost]) -> String {
    layout(
        "Create an account",
        &format!(
            r#"<p class="kicker">Hosted knot</p>
<h1>Create an account.</h1>
<p class="lead">You don’t run a server. This host keeps your posts in a signed repository and gives you a name and a password.</p>
{error}
<form method="post" action="/signup">
  <label>Email <input name="email" type="email" autocomplete="email" required></label>
  <label>Name <input name="name" type="text" maxlength="40" autocomplete="nickname" required></label>
  <label>Password <input name="password" type="password" minlength="8" autocomplete="new-password" required></label>
  <button type="submit">Create account</button>
</form>
<p class="aside">This host generates your key and holds it, so it can write as you. The knot advertises that (<span class="mono">custodialSign</span>). <a href="/login">Sign in</a> if you already have an account.</p>
{posts}"#,
            error = error_block(error),
            posts = post_list(posts),
        ),
    )
}

pub fn login(error: Option<&str>) -> String {
    layout(
        "Sign in",
        &format!(
            r#"<p class="kicker">Hosted knot</p>
<h1>Sign in.</h1>
{error}
<form method="post" action="/login">
  <label>Email <input name="email" type="email" autocomplete="email" required></label>
  <label>Password <input name="password" type="password" autocomplete="current-password" required></label>
  <button type="submit">Sign in</button>
</form>
<p class="aside"><a href="/">Create an account</a></p>"#,
            error = error_block(error),
        ),
    )
}

pub fn home(name: &str, did: &str, error: Option<&str>, posts: &[FeedPost]) -> String {
    layout(
        "Home",
        &format!(
            r#"<p class="kicker">Your account</p>
<h1>{name}</h1>
<p class="did">{did}</p>
{error}
<form method="post" action="/posts">
  <label>Post <textarea name="text" maxlength="2000" required placeholder="Write something public"></textarea></label>
  <button type="submit">Post</button>
</form>
{posts}
<p class="aside"><a href="/imports">Your archive &amp; connections</a> · <a href="/move">Change host</a></p>
<form method="post" action="/logout"><button class="ghost" type="submit">Sign out</button></form>"#,
            name = esc(name),
            did = esc(did),
            error = error_block(error),
            posts = post_list(posts),
        ),
    )
}

pub fn move_start(error: Option<&str>) -> String {
    layout(
        "Change host",
        &format!(
            r#"<p class="kicker">Change host</p>
<h1>Move this account.</h1>
<p class="lead">The next host will hold a new key. This host deletes the old one and stops signing.</p>
{error}
<form method="post" action="/migrate/start">
  <button type="submit">Create a move code</button>
</form>
<p class="aside"><a href="/">Back</a></p>"#,
            error = error_block(error),
        ),
    )
}

pub fn move_stopped() -> String {
    layout(
        "Change host",
        r#"<p class="kicker">Change host</p>
<h1>This host has stopped signing.</h1>
<p class="lead">The account already moved. Sign in on the new host.</p>"#,
    )
}

pub fn move_code(code: &str, host: &str) -> String {
    layout(
        "Change host",
        &format!(
            r#"<p class="kicker">Change host</p>
<h1>Paste this on the new host.</h1>
<p class="lead">Open Move there and enter this code and this host’s address. The code lasts 15 minutes. After the move, this host cannot sign for you.</p>
<p class="did" id="move-code">{code}</p>
<p class="did" id="move-host">{host}</p>
<p class="aside"><a href="/">Back</a></p>"#,
            code = esc(code),
            host = esc(host),
        ),
    )
}

pub fn move_in(error: Option<&str>) -> String {
    layout(
        "Move",
        &format!(
            r#"<p class="kicker">Change host</p>
<h1>Move an account here.</h1>
<p class="lead">Use the code from the host you are leaving. You choose a new password. That host stops signing.</p>
{error}
<form method="post" action="/move">
  <label>Current host <input name="knot" type="url" placeholder="https://old.example" required></label>
  <label>Move code <input name="code" type="text" autocomplete="one-time-code" required></label>
  <label>New password <input name="password" type="password" minlength="8" autocomplete="new-password" required></label>
  <button type="submit">Move here</button>
</form>
<p class="aside"><a href="/">Create an account</a> instead.</p>"#,
            error = error_block(error),
        ),
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
        Some(message) => format!(r#"<p class="error" role="alert">{m}</p>"#, m = esc(message)),
        None => String::new(),
    }
}

fn layout(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} — Tangled</title>
  <style>
    :root {{ color-scheme: dark; }}
    body {{
      margin: 0;
      background: #12141a;
      color: #e6e8ee;
      font-family: "Iowan Old Style", Palatino, Georgia, serif;
      font-size: 17px;
      line-height: 1.55;
    }}
    main {{ max-width: 38rem; margin: 0 auto; padding: 4rem 1.25rem 5rem; }}
    h1 {{
      font-weight: 400;
      font-size: clamp(2.4rem, 6vw, 3.4rem);
      line-height: 1.05;
      letter-spacing: -0.03em;
      margin: 0 0 0.6rem;
    }}
    .kicker {{
      font-family: "Fira Sans", ui-sans-serif, system-ui, sans-serif;
      font-size: 12px;
      font-weight: 600;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      color: #c47a4a;
      margin: 0 0 0.75rem;
    }}
    .lead, .aside, .empty {{ color: #9aa3ad; }}
    .did, .mono {{
      font-family: "Source Code Pro", ui-monospace, Menlo, monospace;
      font-size: 13px;
      color: #9aa3ad;
      word-break: break-all;
    }}
    a {{ color: #e6e8ee; }}
    form {{ display: flex; flex-direction: column; gap: 0.75rem; margin: 1.5rem 0; }}
    label {{
      display: flex;
      flex-direction: column;
      gap: 0.35rem;
      font-family: "Fira Sans", ui-sans-serif, system-ui, sans-serif;
      font-size: 14px;
    }}
    input, textarea {{
      background: #1a1d24;
      color: #e6e8ee;
      border: 1px solid #2a2f38;
      border-radius: 2px;
      padding: 0.6rem 0.7rem;
      font: inherit;
    }}
    textarea {{ min-height: 6rem; resize: vertical; }}
    button {{
      align-self: flex-start;
      background: #c47a4a;
      color: #12141a;
      border: 0;
      border-radius: 2px;
      padding: 10px 18px;
      font-family: "Fira Sans", ui-sans-serif, system-ui, sans-serif;
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
    }}
    button.ghost {{ background: transparent; color: #e6e8ee; border: 1px solid #3d4450; }}
    .error {{ color: #c45c4a; }}
    ol.posts {{ list-style: none; padding: 0; margin: 2rem 0; }}
    ol.posts li {{ border-top: 1px solid #2a2f38; padding: 1rem 0; }}
    ol.posts .name {{
      font-family: "Fira Sans", ui-sans-serif, system-ui, sans-serif;
      font-weight: 500;
      font-size: 16px;
    }}
    ol.posts p {{ margin: 0.35rem 0 0; }}
  </style>
</head>
<body>
  <main>{body}</main>
</body>
</html>"#,
        title = esc(title),
        body = body,
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
