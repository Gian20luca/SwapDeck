//! Segnaposto dell'app desktop: Tauri 2 arriva con lo step 4.

fn banner() -> String {
    format!("swapdeck {}", swapdeck_core::version())
}

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_contains_version() {
        assert_eq!(banner(), format!("swapdeck {}", swapdeck_core::version()));
    }
}
