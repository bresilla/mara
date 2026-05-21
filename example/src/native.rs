use mara_example::DemoApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    mara::window::run::<DemoApp>()
}
