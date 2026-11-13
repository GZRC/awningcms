use crate::error::{ApiError, ApiResult};

pub fn bounded(value: &str, label: &str, min: usize, max: usize) -> ApiResult<()> {
    let size = value.chars().count();
    if size < min || size > max || value.contains('\0') {
        return Err(ApiError::invalid(format!(
            "{label} doit contenir entre {min} et {max} caractères."
        )));
    }
    Ok(())
}
pub fn slug(value: &str) -> ApiResult<()> {
    if value.is_empty()
        || value.len() > 100
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || ["admin", "api", "assets", "healthz"].contains(&value)
    {
        return Err(ApiError::invalid("L'adresse doit utiliser des lettres minuscules, des chiffres et des tirets, sur 100 caractères maximum."));
    }
    Ok(())
}
/// Derives a site address from its name: lowercase ASCII letters and digits,
/// accents folded, words joined by single dashes.
pub fn site_slug(name: &str) -> ApiResult<String> {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        let folded = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
            'ç' => "c",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ñ' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'ý' | 'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            'ß' => "ss",
            c if c.is_ascii_alphanumeric() => {
                slug.push(c);
                continue;
            }
            _ => "-",
        };
        if folded == "-" {
            if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        } else {
            slug.push_str(folded);
        }
    }
    let slug: String = slug.chars().take(60).collect();
    let slug = slug.trim_matches('-').to_string();
    self::slug(&slug)?;
    Ok(slug)
}
pub fn email(value: &str) -> ApiResult<String> {
    let email = value.trim().to_ascii_lowercase();
    let parts: Vec<&str> = email.split('@').collect();
    if email.len() > 254
        || parts.len() != 2
        || parts[0].is_empty()
        || parts[0].len() > 64
        || !email.is_ascii()
    {
        return Err(ApiError::invalid("Renseignez une adresse e-mail valide."));
    }
    let local = parts[0];
    if local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        || domain(parts[1]).is_err()
    {
        return Err(ApiError::invalid("Renseignez une adresse e-mail valide."));
    }
    Ok(email)
}
pub fn password(value: &str) -> ApiResult<()> {
    bounded(value, "Le mot de passe", 12, 256)
}
pub fn domain(value: &str) -> ApiResult<()> {
    if value.len() > 253
        || !value.contains('.')
        || !value.is_ascii()
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(ApiError::invalid(
            "Renseignez un nom de domaine sans https:// ni chemin.",
        ));
    }
    Ok(())
}
pub fn origin(value: &str) -> ApiResult<()> {
    let invalid =
        || ApiError::invalid("L'origine doit être une adresse https exacte, sans chemin.");
    let url = url::Url::parse(value).map_err(|_| invalid())?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || value.ends_with('/')
        || url.host_str().is_none()
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn site_slug_folds_accents_and_spaces() {
        assert_eq!(
            site_slug("Boulangerie Émile & Fils").unwrap(),
            "boulangerie-emile-fils"
        );
        assert_eq!(site_slug("  Café des Arts ").unwrap(), "cafe-des-arts");
        assert!(site_slug("  ").is_err());
        assert!(site_slug("Admin").is_err());
    }
    #[test]
    fn rejects_slug_traversal_reserved_and_markup() {
        for value in [
            "../admin",
            "Admin",
            "admin",
            "foo/bar",
            "<svg>",
            "deux--mots",
            "-page",
            "page-",
        ] {
            assert!(slug(value).is_err(), "{value}");
        }
        assert!(slug("a-propos-2026").is_ok());
    }
    #[test]
    fn origin_requires_exact_secure_origin() {
        for value in [
            "http://example.com",
            "https://example.com/",
            "https://example.com/path",
            "https://user:pass@example.com",
            "https://example.com?x=1",
            "https://example.com#x",
        ] {
            assert!(origin(value).is_err(), "{value}");
        }
        assert!(origin("https://cms.example.com").is_ok());
        assert!(origin("http://127.0.0.1:8080").is_ok());
    }
    #[test]
    fn limits_are_unicode_aware_and_reject_nul() {
        assert!(bounded("éé", "Titre", 1, 2).is_ok());
        assert!(bounded("a\0b", "Titre", 1, 10).is_err());
        assert!(password("court").is_err());
    }
    #[test]
    fn email_and_domain_reject_paths_markup_and_bad_labels() {
        for value in [
            "x@<example.com>",
            "x@example.com/path",
            "a..b@example.com",
            "a@-example.com",
            "a@example..com",
            "bad name@example.com",
        ] {
            assert!(email(value).is_err(), "{value}");
        }
        assert_eq!(
            email(" Owner+cms@Example.com ").ok().as_deref(),
            Some("owner+cms@example.com")
        );
        assert!(domain("cms.example.com").is_ok());
        assert!(domain("https://cms.example.com").is_err());
    }
}
