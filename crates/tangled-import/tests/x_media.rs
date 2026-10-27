use std::io::{Cursor, Write};

use tangled_import::parse;
use zip::write::SimpleFileOptions;

fn archive(data: &str, media: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("data/tweets.js", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(data.as_bytes()).unwrap();
    for (name, bytes) in media {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn video_links_a_supplied_mp4_variant_even_when_the_thumbnail_and_larger_variant_are_absent() {
    let data = r#"window.YTD.tweets.part0 = [{"tweet":{"id_str":"123","full_text":"video","extended_entities":{"media":[{"type":"video","media_url_https":"https://pbs.twimg.com/media/thumb.jpg","video_info":{"variants":[{"content_type":"application/x-mpegURL","url":"https://video.twimg.com/clip.m3u8"},{"content_type":"video/mp4","bitrate":2000,"url":"https://video.twimg.com/large.mp4?tag=12"},{"content_type":"video/mp4","bitrate":500,"url":"https://video.twimg.com/clip.mp4?tag=12"}]}}]}}}];"#;
    let bytes = archive(data, &[("data/tweets_media/123-clip.mp4", b"video")]);
    let batch = parse("twitter", "42", "archive.zip", &bytes).unwrap();
    assert_eq!(
        batch.objects[0].attachments,
        ["data/tweets_media/123-clip.mp4"]
    );
}

#[test]
fn animated_gif_links_the_animation_instead_of_only_its_supplied_poster() {
    let data = r#"window.YTD.tweets.part0 = [{"tweet":{"id_str":"456","full_text":"animation","entities":{"media":[{"type":"animated_gif","media_url_https":"https://pbs.twimg.com/media/poster.jpg"}]},"extended_entities":{"media":[{"type":"animated_gif","media_url_https":"https://pbs.twimg.com/media/poster.jpg","video_info":{"variants":[{"content_type":"video/mp4","url":"https://video.twimg.com/animation.mp4"}]}}]}}}];"#;
    let bytes = archive(
        data,
        &[
            ("data/tweets_media/456-poster.jpg", b"poster"),
            ("data/tweets_media/456-animation.mp4", b"animation"),
        ],
    );
    let batch = parse("twitter", "42", "archive.zip", &bytes).unwrap();
    assert!(batch.objects[0]
        .attachments
        .contains(&"data/tweets_media/456-animation.mp4".to_string()));
    assert!(
        batch.warnings.is_empty(),
        "Supplied animation must not be reported missing: {:?}",
        batch.warnings
    );
}

#[test]
fn missing_media_is_reported_instead_of_silently_disappearing() {
    let data = r#"window.YTD.tweets.part0 = [{"tweet":{"id_str":"789","full_text":"photo","entities":{"media":[{"type":"photo","media_url_https":"https://pbs.twimg.com/media/missing.jpg"}]}}}];"#;
    let bytes = archive(data, &[]);
    let batch = parse("twitter", "42", "archive.zip", &bytes).unwrap();
    assert!(batch
        .warnings
        .iter()
        .any(|warning| { warning.contains("789") && warning.contains("missing.jpg") }));
}
