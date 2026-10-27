use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::{Cursor, Write};
use tangled_import::parse;
use zip::{write::SimpleFileOptions, ZipWriter};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

const TELEGRAM: &[u8] = br#"{"personal_information":{"user_id":42,"first_name":"Alice"},"unknown":{"kept":true},"chats":{"list":[{"id":7,"name":"Friend","messages":[{"id":11,"type":"message","date":"2026-10-27T12:30:00","date_unixtime":"1793100600","from":"Bob","from_id":"user99","text":["hi ",{"type":"bold","text":"Alice"}],"photo":"photos/pic.jpg","future_field":5},{"id":12,"type":"message","date":"2026-10-27T12:30:00","from_id":"user99","text":"again"}]}]}}"#;

#[test]
fn telegram_keeps_native_ids_foreign_senders_rich_text_and_unknown_data() {
    let batch = parse("telegram", "42", "result.json", TELEGRAM).unwrap();
    let post = batch.objects.iter().find(|o| o.kind == "message").unwrap();
    assert_eq!(post.id, "7:11");
    assert_eq!(post.sender.as_deref(), Some("user99"));
    assert_eq!(post.timestamp.as_deref(), Some("2026-10-27T12:30:00"));
    assert_eq!(post.text.as_deref(), Some("hi Alice"));
    assert_eq!(post.data["future_field"], 5);
    assert!(batch
        .objects
        .iter()
        .any(|o| o.data["unknown"]["kept"] == true));
}

#[test]
fn telegram_zip_preserves_attachment_bytes_and_account_mismatch() {
    let input = archive(&[("result.json", TELEGRAM), ("photos/pic.jpg", b"photo")]);
    let batch = parse("telegram", "wrong", "telegram.zip", &input).unwrap();
    assert_eq!(batch.account, "42");
    assert!(batch.warnings.iter().any(|s| s.contains("account")));
    let file = batch
        .files
        .iter()
        .find(|f| f.name == "photos/pic.jpg")
        .unwrap();
    assert_eq!(STANDARD.decode(&file.data_base64).unwrap(), b"photo");
    assert_eq!(file.media_type, "image/jpeg");
    assert!(batch
        .objects
        .iter()
        .any(|o| o.attachments.contains(&"photos/pic.jpg".to_owned())));
}

#[test]
fn telegram_single_chat_and_service_records_are_retained() {
    let batch = parse("telegram", "alice", "result.json", br#"{"id":9,"messages":[{"id":1,"type":"service","action":"invite_members","actor_id":"user7","date":"2025-01-01T00:00:00"}]}"#).unwrap();
    assert!(batch
        .objects
        .iter()
        .any(|o| o.id == "9:1" && o.data["action"] == "invite_members"));
}

#[test]
fn x_archive_maps_posts_direct_messages_relations_and_unknown_datasets() {
    let input = archive(&[
        ("data/account.js", br#"window.YTD.account.part0 = [{"account":{"accountId":"42","username":"alice"}}];"#),
        ("data/tweets.js", br#"window.YTD.tweets.part0 = [{"tweet":{"id_str":"100","full_text":"hello","created_at":"Tue Oct 27 12:00:00 +0000 2026","future":true,"entities":{"media":[{"media_url_https":"https://pbs.twimg.com/media/pic.jpg"}]}}}];"#),
        ("data/direct-messages.js", br#"window.YTD.direct_messages.part0 = [{"dmConversation":{"conversationId":"42-99","messages":[{"messageCreate":{"id":"200","senderId":"99","createdAt":"2026-10-27T12:01:00Z","text":"private"}}]}}];"#),
        ("data/following.js", br#"window.YTD.following.part0 = [{"following":{"accountId":"99"}}];"#),
        ("data/like.js", br#"window.YTD.like.part0 = [{"like":{"tweetId":"101","fullText":"liked"}}];"#),
        ("data/bookmark.js", br#"window.YTD.bookmark.part0 = [{"bookmark":{"tweetId":"102"}}];"#),
        ("data/future.js", br#"window.YTD.future.part0 = [{"anything":{"opaque":7}}];"#),
        ("data/tweets_media/100-pic.jpg", b"pixels"),
    ]);
    let batch = parse("twitter", "42", "archive.zip", &input).unwrap();
    let post = batch.objects.iter().find(|o| o.kind == "post").unwrap();
    assert_eq!(post.id, "100");
    assert_eq!(post.data["future"], true);
    assert_eq!(post.attachments, ["data/tweets_media/100-pic.jpg"]);
    let dm = batch.objects.iter().find(|o| o.kind == "message").unwrap();
    assert_eq!(dm.id, "200");
    assert_eq!(dm.sender.as_deref(), Some("99"));
    assert_eq!(dm.conversation.as_deref(), Some("42-99"));
    for kind in ["following", "like", "bookmark", "archive_data"] {
        assert!(batch.objects.iter().any(|o| o.kind == kind), "{kind}");
    }
}

#[test]
fn x_rejects_executable_wrappers_and_trailing_statements() {
    for body in [
        b"window.YTD.tweets.part0 = []; alert('bad');".as_slice(),
        b"window.YTD.tweets.part0 = (() => [])();".as_slice(),
        b"globalThis.evil = [];".as_slice(),
    ] {
        let input = archive(&[("data/tweets.js", body)]);
        assert!(parse("twitter", "42", "archive.zip", &input).is_err());
    }
}

#[test]
fn whatsapp_android_multiline_and_identical_messages_get_distinct_stable_ids() {
    let text = b"10/27/26, 9:30 AM - Bob: Hello\nsecond line\n10/27/26, 9:31 AM - Bob: yes\n10/27/26, 9:31 AM - Bob: yes\n";
    let batch = parse("whatsapp", "alice", "WhatsApp Chat with Bob.txt", text).unwrap();
    assert_eq!(batch.objects.len(), 3);
    assert_eq!(batch.objects[0].text.as_deref(), Some("Hello\nsecond line"));
    assert_eq!(batch.objects[0].sender.as_deref(), Some("Bob"));
    assert_eq!(
        batch.objects[0].timestamp.as_deref(),
        Some("10/27/26, 9:30 AM")
    );
    assert_ne!(batch.objects[1].id, batch.objects[2].id);
    assert_eq!(
        batch,
        parse("whatsapp", "alice", "WhatsApp Chat with Bob.txt", text).unwrap()
    );
}

#[test]
fn whatsapp_ios_zip_keeps_multiple_chats_and_media() {
    let input = archive(&[
        ("Bob/_chat.txt", "\u{200e}[27/10/2026, 09:30:00] Bob: <attached: 0001-PHOTO.jpg>\n[27/10/2026, 09:31:00] Messages are end-to-end encrypted\n".as_bytes()),
        ("Bob/0001-PHOTO.jpg", b"pixels"),
        ("Carol/_chat.txt", b"[27/10/2026, 09:30:00] Carol: hello\n"),
    ]);
    let batch = parse("whatsapp", "alice", "chats.zip", &input).unwrap();
    assert_eq!(batch.objects.len(), 3);
    assert_ne!(batch.objects[0].conversation, batch.objects[2].conversation);
    assert!(batch
        .objects
        .iter()
        .any(|o| o.attachments == ["Bob/0001-PHOTO.jpg"]));
    assert!(batch
        .objects
        .iter()
        .any(|o| o.kind == "system" && o.sender.is_none()));
}

#[test]
fn whatsapp_utf16_is_decoded_without_losing_unicode() {
    let text = "[27/10/2026, 09:30:00] Bob: héllo 😀\n";
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    let batch = parse("whatsapp", "alice", "chat.txt", &bytes).unwrap();
    assert_eq!(batch.objects[0].text.as_deref(), Some("héllo 😀"));
}

#[test]
fn rejects_zip_traversal_and_platform_unsafe_paths() {
    for name in ["../evil", "/evil", "C:/evil", "a/../../evil", "a\\evil"] {
        let input = archive(&[("result.json", TELEGRAM), (name, b"bad")]);
        assert!(
            parse("telegram", "42", "archive.zip", &input).is_err(),
            "{name}"
        );
    }
}

#[test]
fn rejects_symlinks_and_suspicious_compression_ratios() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("link", "../outside", SimpleFileOptions::default())
        .unwrap();
    let input = writer.finish().unwrap().into_inner();
    assert!(parse("telegram", "42", "archive.zip", &input)
        .unwrap_err()
        .contains("symlink"));
    let input = archive(&[
        ("result.json", TELEGRAM),
        ("bomb", &vec![0; 2 * 1024 * 1024]),
    ]);
    assert!(parse("telegram", "42", "archive.zip", &input)
        .unwrap_err()
        .contains("compression"));
}

#[test]
fn malformed_and_unrecognized_inputs_fail_helpfully() {
    for (source, filename, bytes) in [
        ("telegram", "result.json", b"{invalid".as_slice()),
        ("telegram", "result.json", b"{}".as_slice()),
        ("whatsapp", "chat.txt", b"not a WhatsApp export".as_slice()),
        ("twitter", "archive.zip", b"PK\x03\x04truncated".as_slice()),
        ("unknown", "data", b"anything".as_slice()),
    ] {
        let err = parse(source, "alice", filename, bytes).unwrap_err();
        assert!(!err.is_empty());
    }
}

#[test]
fn x_multipart_posts_use_native_ids_and_ignore_media_json_as_datasets() {
    let input = archive(&[
        (
            "twitter-export/data/tweets-part1.js",
            br#"window.YTD.tweets.part1 = [{"tweet":{"id_str":"103","full_text":"part one"}}];"#,
        ),
        (
            "twitter-export/data/tweets_media/file.json",
            b"not a dataset",
        ),
    ]);
    let batch = parse("twitter", "42", "archive.zip", &input).unwrap();
    assert!(batch
        .objects
        .iter()
        .any(|o| o.id == "103" && o.kind == "post"));
    assert!(batch.files.iter().any(|f| f.name.ends_with("file.json")));
}

#[test]
fn telegram_json_attachments_do_not_break_export_detection() {
    let input = archive(&[
        ("export/result.json", TELEGRAM),
        (
            "export/files/payload.json",
            b"not an export, preserve as bytes",
        ),
        ("export/photos/pic.jpg", b"photo"),
    ]);
    let batch = parse("telegram", "42", "archive.zip", &input).unwrap();
    assert!(batch
        .objects
        .iter()
        .any(|o| o.id == "7:11" && o.attachments == ["export/photos/pic.jpg"]));
    assert!(batch
        .files
        .iter()
        .any(|f| f.name == "export/files/payload.json"));
}

#[test]
fn fallback_ids_preserve_identical_telegram_records() {
    let input = br#"{"id":9,"messages":[{"text":"yes"},{"text":"yes"}]}"#;
    let batch = parse("telegram", "alice", "result.json", input).unwrap();
    let messages: Vec<_> = batch
        .objects
        .iter()
        .filter(|o| o.kind == "message")
        .collect();
    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].id, messages[1].id);
}

#[test]
fn rejects_zip_entry_and_file_limits_before_expansion() {
    let input = archive(&[
        ("result.json", TELEGRAM),
        ("oversized", &vec![0; 33 * 1024 * 1024]),
    ]);
    assert!(parse("telegram", "42", "archive.zip", &input)
        .unwrap_err()
        .contains("32 MiB"));
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..4097 {
        writer
            .start_file(format!("f{i}"), SimpleFileOptions::default())
            .unwrap();
    }
    let input = writer.finish().unwrap().into_inner();
    assert!(parse("telegram", "42", "archive.zip", &input)
        .unwrap_err()
        .contains("4096"));
}

#[test]
fn rejects_duplicate_zip_names_instead_of_silently_overwriting() {
    let mut input = archive(&[("one.txt", b"first"), ("two.txt", b"second")]);
    // ZIP writers prevent duplicate names; alter both same-length headers.
    for offset in 0..input.len() - 7 {
        if &input[offset..offset + 7] == b"two.txt" {
            input[offset..offset + 7].copy_from_slice(b"one.txt");
        }
    }
    let err = parse("whatsapp", "alice", "archive.zip", &input).unwrap_err();
    assert!(err.contains("Duplicate"), "{err}");
}

#[test]
fn x_unknown_datasets_with_same_native_id_do_not_overwrite_each_other() {
    let input = archive(&[
        (
            "data/one.js",
            br#"window.YTD.one.part0 = [{"id":"1","different":"one"}];"#,
        ),
        (
            "data/two.js",
            br#"window.YTD.two.part0 = [{"id":"1","different":"two"}];"#,
        ),
    ]);
    let batch = parse("twitter", "42", "archive.zip", &input).unwrap();
    assert_eq!(batch.objects.len(), 2);
    assert_ne!(batch.objects[0].id, batch.objects[1].id);
}
