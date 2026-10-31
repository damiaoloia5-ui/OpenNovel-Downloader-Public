//! Opt-in network research, separate from normal CI and the user's library.
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [channel, book, chapter] = args.as_slice() else {
        eprintln!("用法：fanqie_protocol_probe <web-api|app-content> <作品ID> <章节ID>");
        std::process::exit(2);
    };
    match opennovel_core::sources::probe_official_chapter(channel, book, chapter) {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("serializable report")
            );
            // A candidate is still not end-to-end extraction acceptance.
            if report.outcome != "readable_candidate" {
                std::process::exit(3);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
