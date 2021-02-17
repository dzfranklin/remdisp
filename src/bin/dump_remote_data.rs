use remdisp::remote::RemoteConfig;

fn main() {
    let remote = RemoteConfig::get();
    println!("{:?}", remote);
}
