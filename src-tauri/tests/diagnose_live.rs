#[tokio::test]
#[ignore = "probes this machine's router, DNS server and Cloudflare"]
async fn live_diagnosis() {
    use weir_lib::core::{
        backends::cloudflare::Cloudflare, diagnose::diagnose, network::Path, probe::readings,
    };
    let path = Path {
        online: true,
        wifi: true,
        ..Path::default()
    };
    let readings = readings(&Cloudflare::default(), path).await;
    let diagnosis = diagnose(&readings, 0);
    println!("HEADLINE {}", diagnosis.headline);
    for step in diagnosis.steps {
        println!("STEP {} {:?} {}", step.label, step.status, step.text);
    }
}
