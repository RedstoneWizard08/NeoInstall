pub fn maven_to_path(mvn: impl AsRef<str>) -> String {
    let mut parts = mvn.as_ref().split('@');
    let first = parts.next().unwrap().to_string();
    let ext = parts.next().map(|v| v.to_string()).unwrap_or("jar".into());
    let mut parts = first.split(':');
    let group = parts.next().unwrap().replace(".", "/");
    let artifact = parts.next().unwrap();
    let version = parts.next().unwrap();
    let classifier = parts.next().map(|v| format!("-{}", v)).unwrap_or_default();

    format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}")
}
