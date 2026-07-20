//! `just run pokedex` — the LIVE leg: ureq (dev-dep) as the transport.
use lumen_core::geometry::Size;
use lumen_shell::RunExt;

fn main() {
    pokedex::app_with(|url| {
        ureq::get(url)
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())
    })
    .run(Size::new(420.0, 320.0));
}
