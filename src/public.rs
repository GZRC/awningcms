//! Public sites are rendered on the server. Published text is escaped before it
//! reaches HTML; neither drafts nor client-supplied HTML are rendered as markup.
use crate::{
    builder::{
        self, Align, BackgroundColor, BuiltinPage, Button, DeviceStyle, FontFamily, FontWeight,
        FooterModel, HeaderModel, MenuLink, MenuTarget, PageDocument, Section, SectionStyle,
        Spacing, TemplateDocument, TextColor, Theme,
    },
    error::{ApiError, ApiResult},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct Site {
    pub(crate) id: Uuid,
    pub(crate) slug: String,
    pub(crate) site_name: String,
    pub(crate) description: String,
    is_primary: bool,
}
struct SiteContext {
    site: Site,
    base: String,
    canonical_origin: String,
    template: Option<TemplateDocument>,
}

#[derive(Debug, PartialEq)]
struct RequestHost {
    hostname: String,
    port: Option<u16>,
}

impl RequestHost {
    fn parse(value: &str) -> ApiResult<Self> {
        if value.is_empty()
            || !value.is_ascii()
            || value
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            || value.contains(['/', '\\', '@', '?', '#', '%', ','])
        {
            return Err(ApiError::missing());
        }
        let (hostname, suffix) = if value.starts_with('[') {
            let (address, suffix) = value.split_once(']').ok_or_else(ApiError::missing)?;
            let address: std::net::Ipv6Addr =
                address[1..].parse().map_err(|_| ApiError::missing())?;
            (format!("[{address}]"), suffix)
        } else {
            let (hostname, suffix) = value
                .split_once(':')
                .map_or((value, ""), |(host, _)| (host, &value[host.len()..]));
            let hostname = hostname.to_ascii_lowercase();
            if hostname != "localhost" && crate::validation::domain(&hostname).is_err() {
                return Err(ApiError::missing());
            }
            (hostname, suffix)
        };
        let port = if suffix.is_empty() {
            None
        } else {
            let port = suffix.strip_prefix(':').ok_or_else(ApiError::missing)?;
            if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(ApiError::missing());
            }
            let port = port.parse::<u16>().map_err(|_| ApiError::missing())?;
            if port == 0 {
                return Err(ApiError::missing());
            }
            Some(port)
        };
        Ok(Self { hostname, port })
    }

    fn from_headers(headers: &HeaderMap) -> ApiResult<Self> {
        let mut hosts = headers.get_all(header::HOST).iter();
        let value = hosts.next().ok_or_else(ApiError::missing)?;
        if hosts.next().is_some() {
            return Err(ApiError::missing());
        }
        Self::parse(value.to_str().map_err(|_| ApiError::missing())?)
    }

    fn matches_origin(&self, origin: &str) -> bool {
        let Ok(origin) = url::Url::parse(origin) else {
            return false;
        };
        let default_port = match origin.scheme() {
            "https" => Some(443),
            "http" => Some(80),
            _ => None,
        };
        origin
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(&self.hostname))
            && self.port.or(default_port) == origin.port_or_known_default()
    }
}

fn configured_host(state: &AppState, host: &RequestHost) -> bool {
    host.matches_origin(&state.origin)
        || host.matches_origin(&state.primary_site_origin)
        || primary_apex_origin(&state.primary_site_origin, host).is_some()
}

fn primary_apex_origin(primary_origin: &str, host: &RequestHost) -> Option<String> {
    let origin = url::Url::parse(primary_origin).ok()?;
    if origin.scheme() != "https"
        || origin.port_or_known_default() != Some(443)
        || host.port.is_some_and(|port| port != 443)
    {
        return None;
    }
    let apex = origin.host_str()?.strip_prefix("www.")?;
    if apex != host.hostname {
        return None;
    }
    Some(format!("https://{apex}"))
}

async fn resolve_site(
    state: &AppState,
    headers: &HeaderMap,
    slug: Option<&str>,
) -> ApiResult<SiteContext> {
    let host = RequestHost::from_headers(headers)?;
    if slug.is_some() && !host.matches_origin(&state.origin) {
        return Err(ApiError::missing());
    }
    if configured_host(state, &host) {
        let site = site(state, slug).await?;
        let base = base(&site, slug.is_some());
        let canonical_origin = if slug.is_some() {
            state.origin.as_ref().clone()
        } else {
            state.primary_site_origin.as_ref().clone()
        };
        return Ok(SiteContext {
            site,
            base,
            canonical_origin,
            template: None,
        });
    }
    // A client hostname has no fallback to the primary site. Only a verified,
    // still-allowed binding decides which site's published content it receives.
    if host.port.is_some_and(|port| port != 443) {
        return Err(ApiError::missing());
    }
    let binding = crate::domains::binding_for_host(state, &host.hostname)
        .await?
        .ok_or_else(ApiError::missing)?;
    let site = sqlx::query_as("SELECT s.id,s.slug,c.site_name,c.description,s.is_primary FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE s.id=$1")
        .bind(binding.site_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
    Ok(SiteContext {
        site,
        base: String::new(),
        canonical_origin: format!("https://{}", binding.domain),
        template: None,
    })
}

pub(crate) async fn resolve_host_site(state: &AppState, headers: &HeaderMap) -> ApiResult<Site> {
    Ok(resolve_site(state, headers, None).await?.site)
}

pub(crate) async fn resolve_extranet_context(
    state: &AppState,
    headers: &HeaderMap,
) -> ApiResult<(Site, String)> {
    let host = RequestHost::from_headers(headers)?;
    let context = resolve_site(state, headers, None).await?;
    // CSRF checks use the origin actually serving this extranet, which differs
    // from the public canonical URL when the central origin hosts test traffic.
    let origin = if host.matches_origin(&state.origin) {
        state.origin.as_ref().clone()
    } else if let Some(apex) = primary_apex_origin(&state.primary_site_origin, &host) {
        apex
    } else {
        context.canonical_origin
    };
    Ok((context.site, origin))
}
#[derive(Deserialize)]
struct Published {
    id: Uuid,
    kind: String,
    title: String,
    slug: String,
    excerpt: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    layout: Option<PageDocument>,
    published_at: DateTime<Utc>,
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}
async fn site(state: &AppState, slug: Option<&str>) -> ApiResult<Site> {
    sqlx::query_as("SELECT s.id,s.slug,c.site_name,c.description,s.is_primary FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE (($1::text IS NULL AND s.is_primary) OR s.slug=$1)")
        .bind(slug).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)
}
async fn contents(state: &AppState, site_id: Uuid) -> ApiResult<Vec<Published>> {
    let values:Vec<serde_json::Value>=sqlx::query_scalar("SELECT c.published_snapshot - 'body' - 'layout' FROM cms_contents c JOIN cms_settings s ON s.site_id=c.site_id WHERE c.site_id=$1 AND c.published_snapshot IS NOT NULL AND (c.kind='page' OR s.blog_enabled) ORDER BY c.published_at DESC,c.id")
        .bind(site_id).fetch_all(&state.pool).await?;
    values
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(|_| ApiError::internal()))
        .collect()
}
fn base(site: &Site, named: bool) -> String {
    if named {
        format!("/s/{}", site.slug)
    } else {
        String::new()
    }
}
fn home_path(base: &str) -> &str {
    if base.is_empty() {
        "/"
    } else {
        base
    }
}
fn content_path(base: &str, content: &Published) -> String {
    if content.kind == "page" && content.slug == "a-propos" {
        return format!("{base}/a-propos");
    }
    format!(
        "{base}/{}/{}",
        if content.kind == "article" {
            "articles"
        } else {
            "pages"
        },
        content.slug
    )
}
fn canonical_url(origin: &str, path: &str) -> String {
    format!("{origin}{path}")
}
fn extranet_shell(site_id: Uuid) -> Response {
    let mut response = Html(include_str!("../static/admin.html")).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-cms-extranet",
        HeaderValue::from_str(&site_id.to_string()).expect("UUID is a valid header"),
    );
    response
}
pub async fn extranet(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match resolve_host_site(&state, &headers).await {
        Ok(site) => extranet_shell(site.id),
        Err(error) => error.into_response(),
    }
}
pub async fn site_extranet(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    match named_client_origin(&state, &headers, &slug).await {
        Ok(origin) => {
            let mut response = Redirect::to(&format!("{origin}/extranet")).into_response();
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => error.into_response(),
    }
}

pub async fn site_account(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    match named_client_origin(&state, &headers, &slug).await {
        Ok(origin) => {
            let mut response = Redirect::to(&format!("{origin}/mon-compte")).into_response();
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => error.into_response(),
    }
}

async fn named_client_origin(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> ApiResult<String> {
    let context = resolve_site(state, headers, Some(slug)).await?;
    let domain: Option<String> = sqlx::query_scalar(
        "SELECT domain FROM cms_domain_bindings WHERE site_id=$1 AND status='connected'",
    )
    .bind(context.site.id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(domain) = domain {
        if crate::domains::binding_for_host(state, &domain)
            .await?
            .is_some_and(|binding| binding.site_id == context.site.id)
        {
            return Ok(format!("https://{domain}"));
        }
    }
    if context.site.is_primary {
        return Ok(state.primary_site_origin.as_ref().clone());
    }
    Err(ApiError::missing())
}
#[derive(Serialize)]
pub(crate) struct NavigationLink {
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) icon: Option<&'static str>,
}
fn header_links(base: &str, items: &[Published]) -> Vec<NavigationLink> {
    let mut links = vec![
        NavigationLink {
            label: "Accueil".into(),
            path: home_path(base).into(),
            icon: None,
        },
        NavigationLink {
            label: "À propos".into(),
            path: format!("{base}/a-propos"),
            icon: None,
        },
        NavigationLink {
            label: "Blog".into(),
            path: format!("{base}/blog"),
            icon: None,
        },
    ];
    links.extend(
        items
            .iter()
            .filter(|item| item.kind == "page" && item.slug != "a-propos")
            .take(3)
            .map(|item| NavigationLink {
                label: item.title.clone(),
                path: content_path(base, item),
                icon: None,
            }),
    );
    links.push(NavigationLink {
        label: "Mon compte".into(),
        path: format!("{base}/mon-compte"),
        icon: Some("user-round"),
    });
    links
}
fn footer_links(base: &str) -> Vec<NavigationLink> {
    vec![
        NavigationLink {
            label: "À propos".into(),
            path: format!("{base}/a-propos"),
            icon: None,
        },
        NavigationLink {
            label: "Blog".into(),
            path: format!("{base}/blog"),
            icon: None,
        },
        NavigationLink {
            label: "Mon compte".into(),
            path: format!("{base}/mon-compte"),
            icon: None,
        },
    ]
}
pub(crate) async fn template_navigation(
    state: &AppState,
    site_id: Uuid,
) -> ApiResult<(Vec<NavigationLink>, Vec<NavigationLink>)> {
    let items = contents(state, site_id).await?;
    if let Some(template) = builder::published_template(state, site_id).await? {
        Ok((
            menu_links("", &items, &template.header.links),
            menu_links("", &items, &template.footer.links),
        ))
    } else {
        Ok((header_links("", &items), footer_links("")))
    }
}

fn menu_links(base: &str, items: &[Published], links: &[MenuLink]) -> Vec<NavigationLink> {
    links
        .iter()
        .filter_map(|link| {
            let (path, icon) = match &link.target {
                MenuTarget::Builtin { page } => match page {
                    BuiltinPage::Home => (home_path(base).to_string(), None),
                    BuiltinPage::About => (format!("{base}/a-propos"), None),
                    BuiltinPage::Blog => (format!("{base}/blog"), None),
                    BuiltinPage::Account => (format!("{base}/mon-compte"), Some("user-round")),
                },
                // Publishing a menu cannot publish a page or reveal a draft slug.
                MenuTarget::Page { id } => (
                    content_path(
                        base,
                        items
                            .iter()
                            .find(|item| item.id == *id && item.kind == "page")?,
                    ),
                    None,
                ),
            };
            Some(NavigationLink {
                label: link.label.clone(),
                path,
                icon,
            })
        })
        .collect()
}

fn navigation_link(href: &str, label: &str, current_path: &str) -> String {
    let current = if href == current_path {
        " aria-current=\"page\""
    } else {
        ""
    };
    format!(
        "<a class=\"nav-link\" href=\"{}\"{current}>{}</a>",
        escape(href),
        escape(label)
    )
}
fn document(
    context: &SiteContext,
    path: &str,
    title: &str,
    description: &str,
    items: &[Published],
    body: &str,
) -> String {
    let site = &context.site;
    let base = &context.base;
    let site_name = escape(&site.site_name);
    let home = escape(home_path(base));
    let footer_items = context
        .template
        .as_ref()
        .map(|template| menu_links(base, items, &template.footer.links))
        .unwrap_or_else(|| footer_links(base));
    let footer_nav: String = footer_items
        .iter()
        .map(|item| {
            format!(
                "<a href=\"{}\">{}</a>",
                escape(&item.path),
                escape(&item.label)
            )
        })
        .collect();
    let mut nav = String::new();
    let header_items = context
        .template
        .as_ref()
        .map(|template| menu_links(base, items, &template.header.links))
        .unwrap_or_else(|| header_links(base, items));
    for item in header_items {
        if item.icon.is_some() {
            nav.push_str(&format!("<a class=\"account-link\" href=\"{}\" aria-label=\"{}\" title=\"{}\"><img class=\"user-icon\" src=\"/assets/icons/user-round.svg\" alt=\"\" width=\"24\" height=\"24\"></a>", escape(&item.path), escape(&item.label), escape(&item.label)));
        } else {
            nav.push_str(&navigation_link(&item.path, &item.label, path));
        }
    }
    let header_model = match context
        .template
        .as_ref()
        .map(|template| &template.header.model)
    {
        Some(HeaderModel::Centered) => "centered",
        _ => "classic",
    };
    let footer_model = match context
        .template
        .as_ref()
        .map(|template| &template.footer.model)
    {
        Some(FooterModel::Columns) => "columns",
        _ => "simple",
    };
    let footer_description = context
        .template
        .as_ref()
        .map(|template| template.footer.description.as_str())
        .unwrap_or("Un espace à découvrir, une histoire à partager.");
    let footer_description = if footer_description.is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"footer-note\">{}</p>",
            escape(footer_description)
        )
    };
    let year = Utc::now().format("%Y");
    format!(
        r##"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light"><title>{}</title><meta name="description" content="{}"><link rel="canonical" href="{}"><link rel="stylesheet" href="/assets/public.css"><link rel="stylesheet" href="/assets/blocks.css?v=20261110-2"></head>
<body><a class="skip-link" href="#main">Aller au contenu</a><header class="site-header" data-template-area="header" data-model="{header_model}"><a class="site-brand" href="{home}"><span class="brand-mark" aria-hidden="true">{}</span><span>{site_name}</span></a><nav class="site-nav" aria-label="Navigation principale">{nav}</nav></header><main id="main">{body}</main><footer class="site-footer" data-template-area="footer" data-model="{footer_model}"><div class="footer-brand"><a href="{home}">{site_name}</a>{footer_description}</div><nav class="footer-nav" aria-label="Navigation de pied de page">{footer_nav}</nav><p class="footer-copyright">© {year} {site_name}</p></footer></body></html>"##,
        escape(title),
        escape(description),
        escape(&canonical_url(&context.canonical_origin, path)),
        escape(
            &site
                .site_name
                .chars()
                .next()
                .unwrap_or('C')
                .to_uppercase()
                .to_string()
        ),
    )
}

pub(crate) async fn render_account(state: &AppState, headers: &HeaderMap) -> ApiResult<String> {
    let mut context = resolve_site(state, headers, None).await?;
    context.template = builder::published_template(state, context.site.id).await?;
    let items = contents(state, context.site.id).await?;
    Ok(account_document(&context, &items))
}

fn account_document(context: &SiteContext, items: &[Published]) -> String {
    document(
        context,
        "/mon-compte",
        &format!("Mon compte · {}", context.site.site_name),
        "Connectez-vous à votre espace personnel ou créez votre compte.",
        items,
        include_str!("../static/account.html"),
    )
    .replacen("</head>", "<meta name=\"robots\" content=\"noindex, nofollow\"><link rel=\"stylesheet\" href=\"/assets/account.css\"><script src=\"/assets/account.js\" defer></script></head>", 1)
    .replacen("<main id=\"main\">", "<main id=\"main\" class=\"account-public-main\">", 1)
}

// The editor stores structured data only. Rendering never accepts HTML, CSS,
// handlers, arbitrary class names, or a caller-controlled heading tag.
fn render_sections(layout: &PageDocument, base: &str) -> String {
    let mut html = String::from("<div class=\"site-blocks\">");
    let mut first_heading = true;
    for section in &layout.sections {
        let (id, align, spacing, theme, kind) = match section {
            Section::Hero {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "hero"),
            Section::Text {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "text"),
            Section::Image {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "image"),
            Section::Columns {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "columns"),
            Section::Cta {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "cta"),
            Section::Heading {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "heading"),
            Section::Button {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "button"),
            Section::Divider {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "divider"),
            Section::Spacer {
                id,
                align,
                spacing,
                theme,
                ..
            } => (id, align, spacing, theme, "spacer"),
        };
        let align = match align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        };
        let spacing = match spacing {
            Spacing::Small => "small",
            Spacing::Medium => "medium",
            Spacing::Large => "large",
        };
        let theme = match theme {
            Theme::Paper => "paper",
            Theme::Sage => "sage",
            Theme::Forest => "forest",
        };
        let widget_class = if kind == "button" {
            "widget-button"
        } else {
            kind
        };
        let style_classes = section_style_classes(section.style());
        html.push_str(&format!("<section class=\"site-block block-{widget_class} block-align-{align} block-space-{spacing} block-theme-{theme}{style_classes}\" data-section-id=\"{id}\" data-section-type=\"{kind}\"><div class=\"block-inner\">"));
        match section {
            Section::Hero {
                eyebrow,
                title,
                text,
                image_url,
                image_alt,
                button,
                ..
            } => {
                let image = image_url.as_deref().filter(|value| safe_image_url(value));
                html.push_str(if image.is_some() {
                    "<div class=\"block-hero-grid has-image\"><div class=\"block-copy\">"
                } else {
                    "<div class=\"block-hero-grid\"><div class=\"block-copy\">"
                });
                if !eyebrow.is_empty() {
                    html.push_str(&format!(
                        "<p class=\"block-eyebrow\" data-edit-field=\"eyebrow\">{}</p>",
                        escape(eyebrow)
                    ));
                }
                html.push_str(&block_heading(title, &mut first_heading));
                html.push_str(&block_text(text, "text"));
                if let Some(button) = button {
                    html.push_str(&block_button(button, base));
                }
                html.push_str("</div>");
                if let Some(url) = image {
                    html.push_str(&format!("<figure class=\"block-hero-image\"><img src=\"{}\" alt=\"{}\" width=\"1200\" height=\"900\" decoding=\"async\" referrerpolicy=\"no-referrer\"></figure>", escape(url), escape(image_alt)));
                }
                html.push_str("</div>");
            }
            Section::Text { title, text, .. } => {
                html.push_str("<div class=\"block-prose\">");
                html.push_str(&block_heading(title, &mut first_heading));
                html.push_str(&block_text(text, "text"));
                html.push_str("</div>");
            }
            Section::Image {
                url, alt, caption, ..
            } => {
                if safe_image_url(url) {
                    html.push_str(&format!("<figure class=\"block-image\"><img src=\"{}\" alt=\"{}\" width=\"1440\" height=\"900\" loading=\"lazy\" decoding=\"async\" referrerpolicy=\"no-referrer\">", escape(url), escape(alt)));
                    if !caption.is_empty() {
                        html.push_str(&format!(
                            "<figcaption data-edit-field=\"caption\">{}</figcaption>",
                            escape(caption)
                        ));
                    }
                    html.push_str("</figure>");
                }
            }
            Section::Columns { title, items, .. } => {
                html.push_str(&block_heading(title, &mut first_heading));
                let count = match items.len() {
                    2 => "two",
                    3 => "three",
                    4 => "four",
                    _ => "one",
                };
                html.push_str(&format!(
                    "<div class=\"block-columns-grid block-columns-{count}\">"
                ));
                for (index, item) in items.iter().enumerate() {
                    html.push_str(&format!("<div class=\"block-column\"><span class=\"block-column-number\" aria-hidden=\"true\">{:02}</span><h3 data-edit-field=\"items.{index}.title\">{}</h3>{}</div>", index + 1, escape(&item.title), block_text(&item.text, &format!("items.{index}.text"))));
                }
                html.push_str("</div>");
            }
            Section::Cta {
                title,
                text,
                button,
                ..
            } => {
                html.push_str("<div class=\"block-cta-copy\">");
                html.push_str(&block_heading(title, &mut first_heading));
                html.push_str(&block_text(text, "text"));
                html.push_str(&block_button(button, base));
                html.push_str("</div>");
            }
            Section::Heading { title, .. } => {
                html.push_str(&block_heading(title, &mut first_heading))
            }
            Section::Button { button, .. } => html.push_str(&block_button(button, base)),
            Section::Divider { .. } => html.push_str("<hr class=\"block-divider-line\">"),
            Section::Spacer { height, .. } => {
                let height = if builder::SPACER_HEIGHTS.contains(height) {
                    *height
                } else {
                    8
                };
                html.push_str(&format!(
                    "<div class=\"block-spacer-height-{height}\" aria-hidden=\"true\"></div>"
                ));
            }
        }
        html.push_str("</div></section>");
    }
    html.push_str("</div>");
    html
}

fn section_style_classes(style: Option<&SectionStyle>) -> String {
    let Some(style) = style else {
        return String::new();
    };
    let mut classes = String::new();
    let background = match style.background {
        BackgroundColor::Default => None,
        BackgroundColor::White => Some("white"),
        BackgroundColor::Cream => Some("cream"),
        BackgroundColor::Sage => Some("sage"),
        BackgroundColor::Forest => Some("forest"),
        BackgroundColor::Ink => Some("ink"),
        BackgroundColor::Purple => Some("purple"),
        BackgroundColor::Blue => Some("blue"),
        BackgroundColor::Rose => Some("rose"),
    };
    if let Some(background) = background {
        classes.push_str(&format!(" bs-bg-{background}"));
    }
    if let Some(color) = text_color_class(style.text_color) {
        classes.push_str(&format!(" bs-text-{color}"));
    }
    let font = match style.font_family {
        FontFamily::Inherit => None,
        FontFamily::Sans => Some("sans"),
        FontFamily::Serif => Some("serif"),
        FontFamily::Mono => Some("mono"),
    };
    if let Some(font) = font {
        classes.push_str(&format!(" bs-font-{font}"));
    }
    if let Some(weight) = style.font_weight {
        let weight = match weight {
            FontWeight::Normal => "normal",
            FontWeight::Medium => "medium",
            FontWeight::Semibold => "semibold",
            FontWeight::Bold => "bold",
        };
        classes.push_str(&format!(" bs-weight-{weight}"));
    }
    numeric_style_class(
        &mut classes,
        "bs-radius",
        style.border_radius,
        builder::BORDER_RADII,
    );
    numeric_style_class(
        &mut classes,
        "bs-border",
        style.border_width,
        builder::BORDER_WIDTHS,
    );
    if let Some(color) = text_color_class(style.border_color) {
        classes.push_str(&format!(" bs-border-color-{color}"));
    }
    for (breakpoint, device) in [
        ("d", &style.desktop),
        ("t", &style.tablet),
        ("m", &style.mobile),
    ] {
        device_style_classes(&mut classes, breakpoint, device);
    }
    for (hidden, name) in [
        (style.hidden_desktop, "desktop"),
        (style.hidden_tablet, "tablet"),
        (style.hidden_mobile, "mobile"),
    ] {
        if hidden {
            classes.push_str(&format!(" bs-hide-{name}"));
        }
    }
    classes
}

fn text_color_class(color: TextColor) -> Option<&'static str> {
    match color {
        TextColor::Default => None,
        TextColor::Ink => Some("ink"),
        TextColor::Muted => Some("muted"),
        TextColor::White => Some("white"),
        TextColor::Forest => Some("forest"),
        TextColor::Purple => Some("purple"),
        TextColor::Blue => Some("blue"),
    }
}

fn numeric_style_class(classes: &mut String, prefix: &str, value: Option<u16>, allowed: &[u16]) {
    // Even a corrupted stored document cannot create an unbounded CSS value.
    if let Some(value) = value.filter(|value| allowed.contains(value)) {
        classes.push_str(&format!(" {prefix}-{value}"));
    }
}

fn device_style_classes(classes: &mut String, breakpoint: &str, device: &DeviceStyle) {
    numeric_style_class(
        classes,
        &format!("bs-{breakpoint}-title"),
        device.title_size,
        builder::FONT_SIZES,
    );
    numeric_style_class(
        classes,
        &format!("bs-{breakpoint}-text"),
        device.text_size,
        builder::FONT_SIZES,
    );
    numeric_style_class(
        classes,
        &format!("bs-{breakpoint}-width"),
        device.content_width,
        builder::CONTENT_WIDTHS,
    );
    for (property, edges) in [
        ("p", device.padding.as_ref()),
        ("m", device.margin.as_ref()),
    ] {
        if let Some(edges) = edges {
            for (edge, value) in [
                ("t", edges.top),
                ("r", edges.right),
                ("b", edges.bottom),
                ("l", edges.left),
            ] {
                numeric_style_class(
                    classes,
                    &format!("bs-{breakpoint}-{property}{edge}"),
                    Some(value),
                    builder::EDGE_SPACING,
                );
            }
        }
    }
    if let Some(align) = device.align {
        let align = match align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        };
        classes.push_str(&format!(" bs-{breakpoint}-align-{align}"));
    }
}

fn block_heading(title: &str, first: &mut bool) -> String {
    if title.is_empty() {
        return String::new();
    }
    let tag = if *first { "h1" } else { "h2" };
    *first = false;
    format!(
        "<{tag} class=\"block-title\" data-edit-field=\"title\">{}</{tag}>",
        escape(title)
    )
}

fn block_text(text: &str, field: &str) -> String {
    let paragraphs: String = text
        .split("\n\n")
        .filter(|text| !text.trim().is_empty())
        .map(|text| format!("<p>{}</p>", escape(text)))
        .collect();
    if paragraphs.is_empty() {
        String::new()
    } else {
        format!("<div class=\"block-richtext\" data-edit-field=\"{}\" data-edit-kind=\"multiline\">{paragraphs}</div>", escape(field))
    }
}

fn safe_local_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && !value.to_ascii_lowercase().contains("%5c")
        && !value.to_ascii_lowercase().contains("%2f")
}

fn safe_image_url(value: &str) -> bool {
    if builder::validate_image(value).is_err() {
        return false;
    }
    if value.starts_with("/assets/") {
        return safe_local_path(value) && !value.contains("..");
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && !value.chars().any(char::is_control)
}

fn block_button(button: &Button, base: &str) -> String {
    if builder::validate_href(&button.href).is_err() {
        return String::new();
    }
    let path = if button.href == "/" {
        home_path(base).to_string()
    } else if safe_local_path(&button.href) {
        format!("{base}{}", button.href)
    } else if button.href.starts_with("https://") {
        button.href.clone()
    } else {
        return String::new();
    };
    format!("<div class=\"block-actions\"><a class=\"public-button block-button\" href=\"{}\"><span class=\"block-button-label\" data-edit-field=\"button.label\">{}</span><span aria-hidden=\"true\">↗</span></a></div>", escape(&path), escape(&button.label))
}

fn cards(base: &str, items: &[Published], kind: &str, label: &str) -> String {
    let matching: Vec<&Published> = items.iter().filter(|item| item.kind == kind).collect();
    if matching.is_empty() {
        return String::new();
    }
    let mut html=format!("<section class=\"content-section\"><div class=\"section-heading\"><h2>{}</h2><span>{}</span></div><div class=\"content-grid\">",escape(label),matching.len());
    for item in matching {
        html.push_str(&format!("<a class=\"content-card\" href=\"{}\"><h3>{}</h3><p>{}</p><span class=\"card-link\">Découvrir <img class=\"link-icon\" src=\"/assets/icons/arrow-up-right.svg\" alt=\"\" width=\"18\" height=\"18\" aria-hidden=\"true\"></span></a>",escape(&content_path(base,item)),escape(&item.title),escape(&item.excerpt)));
    }
    html.push_str("</div></section>");
    html
}
async fn render_home(
    state: &AppState,
    headers: &HeaderMap,
    slug: Option<&str>,
) -> ApiResult<String> {
    let context = resolve_site(state, headers, slug).await?;
    render_home_context(state, context).await
}
async fn render_home_context(state: &AppState, mut context: SiteContext) -> ApiResult<String> {
    if context.template.is_none() {
        context.template = builder::published_template(state, context.site.id).await?;
    }
    let site = &context.site;
    let items = contents(state, site.id).await?;
    let base = &context.base;
    if let Some(layout) = builder::published_home(state, site.id).await? {
        return Ok(document(
            &context,
            home_path(base),
            &format!("Bienvenue — {}", site.site_name),
            &site.description,
            &items,
            &render_sections(&layout, base),
        ));
    }
    let introductory = if site.description.trim().is_empty() {
        if items.is_empty() {
            "Un nouvel espace prend vie. Nous préparons la suite avec soin, et nous avons hâte de vous la faire découvrir."
        } else {
            "Découvrez notre univers, explorez nos pages et retrouvez nos dernières nouvelles."
        }
    } else {
        &site.description
    };
    let badge = if items.is_empty() {
        "Le début d’une belle histoire"
    } else {
        "Bienvenue dans notre univers"
    };
    let illustration = if items.is_empty() {
        "<div class=\"hero-visual\"><img src=\"/assets/site-construction.webp\" alt=\"Un site en cours de construction\" width=\"640\" height=\"640\" fetchpriority=\"high\" decoding=\"async\"></div>"
    } else {
        ""
    };
    let modifier = if items.is_empty() {
        "hero--welcome"
    } else {
        "hero--published"
    };
    let footnote = if items.is_empty() {
        "Prenez vos marques. La suite arrive bientôt."
    } else {
        "Faites comme chez vous. Bonne découverte !"
    };
    let mut body = format!(
        r##"<section class="hero {modifier}"><div class="hero-copy"><p class="welcome-badge">{badge}</p><h1>Bienvenue chez<br><span>{}.</span></h1><p class="lead">{}</p><div class="hero-actions"><a class="public-button primary" href="{}">Faire connaissance</a><a class="public-button secondary" href="{}">Découvrir le blog</a></div><p class="hero-footnote">{footnote}</p></div>{illustration}</section>"##,
        escape(&site.site_name),
        escape(introductory),
        escape(&format!("{base}/a-propos")),
        escape(&format!("{base}/blog"))
    );
    if items.is_empty() {
        body.push_str("<section class=\"construction-notice\" aria-labelledby=\"construction-title\"><span class=\"construction-status\">EN PRÉPARATION</span><div><h2 id=\"construction-title\">Le site est en cours de construction.</h2><p>Nous peaufinons les derniers détails pour vous accueillir comme il se doit. Merci de votre visite et de votre patience. Revenez nous voir bientôt !</p></div></section>");
    }
    body.push_str(&cards(base, &items, "page", "À découvrir"));
    body.push_str(&cards(base, &items, "article", "Le blog"));
    Ok(document(
        &context,
        home_path(base),
        &format!("Bienvenue — {}", site.site_name),
        introductory,
        &items,
        &body,
    ))
}

async fn render_about(
    state: &AppState,
    headers: &HeaderMap,
    slug: Option<&str>,
) -> ApiResult<String> {
    let context = resolve_site(state, headers, slug).await?;
    render_about_context(state, context).await
}
async fn render_about_context(state: &AppState, mut context: SiteContext) -> ApiResult<String> {
    if context.template.is_none() {
        context.template = builder::published_template(state, context.site.id).await?;
    }
    let site = &context.site;
    let items = contents(state, site.id).await?;
    let path = format!("{}/a-propos", context.base);
    // Only a published snapshot can replace this common starter page. Drafts
    // remain private, including edits to an already published about page.
    let published: Option<serde_json::Value> = sqlx::query_scalar("SELECT published_snapshot FROM cms_contents WHERE site_id=$1 AND kind='page' AND published_slug='a-propos' AND published_snapshot IS NOT NULL")
        .bind(site.id).fetch_optional(&state.pool).await?;
    if let Some(value) = published {
        let item: Published = serde_json::from_value(value).map_err(|_| ApiError::internal())?;
        return Ok(content_document(&context, &item, &items, &path));
    }
    let title = format!("À propos de {}", site.site_name);
    let introduction = if site.description.trim().is_empty() {
        "Une nouvelle histoire commence ici."
    } else {
        &site.description
    };
    let body = format!(
        r##"<section class="page-intro"><p class="eyebrow">FAISONS CONNAISSANCE</p><h1>{}</h1><p class="lead">{}</p></section><section class="empty-content about-body"><p class="construction-status">BIENTÔT ICI</p><h2>Notre présentation prend forme.</h2><p>Nous préparons cet espace pour vous faire découvrir {}. Notre histoire, notre univers et nos prochaines nouvelles trouveront bientôt leur place ici.</p><p>Merci de faire partie des premières visites.</p><a class="public-button primary" href="{}">Retour à l’accueil</a></section>"##,
        escape(&title),
        escape(introduction),
        escape(&site.site_name),
        escape(home_path(&context.base))
    );
    Ok(document(
        &context,
        &path,
        &title,
        introduction,
        &items,
        &body,
    ))
}

async fn render_blog(
    state: &AppState,
    headers: &HeaderMap,
    slug: Option<&str>,
) -> ApiResult<String> {
    let context = resolve_site(state, headers, slug).await?;
    render_blog_context(state, context).await
}
async fn render_blog_context(state: &AppState, mut context: SiteContext) -> ApiResult<String> {
    if context.template.is_none() {
        context.template = builder::published_template(state, context.site.id).await?;
    }
    // contents() excludes drafts and articles while the Blog module is disabled.
    let items = contents(state, context.site.id).await?;
    let articles = items.iter().any(|item| item.kind == "article");
    let path = format!("{}/blog", context.base);
    let title = format!("Le blog — {}", context.site.site_name);
    let description = format!("Retrouvez les nouvelles de {}.", context.site.site_name);
    let mut body = format!(
        r##"<section class="page-intro"><p class="eyebrow">LES NOUVELLES DU SITE</p><h1>Le blog.</h1><p class="lead">{}</p></section>"##,
        escape(&description)
    );
    if articles {
        body.push_str(&cards(
            &context.base,
            &items,
            "article",
            "Dernières publications",
        ));
    } else {
        body.push_str(&format!(r##"<section class="empty-content"><p class="construction-status">À SUIVRE</p><h2>Les premières histoires se préparent.</h2><p>Les articles de {} seront à découvrir ici. Revenez bientôt pour lire nos premières publications.</p><a class="public-button primary" href="{}">Retour à l’accueil</a></section>"##, escape(&context.site.site_name), escape(home_path(&context.base))));
    }
    Ok(document(
        &context,
        &path,
        &title,
        &description,
        &items,
        &body,
    ))
}
pub(crate) async fn render_builder_preview(
    state: &AppState,
    site_id: Uuid,
    page: Option<&PageDocument>,
    template: Option<&TemplateDocument>,
    content_id: Option<Uuid>,
) -> ApiResult<String> {
    let site: Site = sqlx::query_as("SELECT s.id,s.slug,c.site_name,c.description,s.is_primary FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE s.id=$1")
        .bind(site_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
    let template = match template {
        Some(template) => Some(template.clone()),
        None => builder::published_template(state, site_id).await?,
    };
    let context = SiteContext {
        site,
        base: String::new(),
        canonical_origin: state.origin.as_ref().clone(),
        template,
    };
    let html = if let Some(page) = page {
        let items = contents(state, site_id).await?;
        let (title, description, path) = if let Some(content_id) = content_id {
            let (title, excerpt, slug): (String, String, String) = sqlx::query_as("SELECT title,excerpt,slug FROM cms_contents WHERE site_id=$1 AND id=$2 AND kind='page'")
                .bind(site_id).bind(content_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
            let path = if slug == "a-propos" {
                "/a-propos".to_string()
            } else {
                format!("/pages/{slug}")
            };
            (
                format!("{title} — {}", context.site.site_name),
                excerpt,
                path,
            )
        } else {
            (
                format!("Bienvenue — {}", context.site.site_name),
                context.site.description.clone(),
                "/".into(),
            )
        };
        document(
            &context,
            &path,
            &title,
            &description,
            &items,
            &render_sections(page, ""),
        )
    } else {
        render_home_context(state, context).await?
    };
    preview_document(&html, "builder", "full")
}

pub(crate) async fn render_template_preview(
    state: &AppState,
    site_id: Uuid,
    page: &str,
    area: &str,
) -> ApiResult<String> {
    let site: Site = sqlx::query_as("SELECT s.id,s.slug,c.site_name,c.description,s.is_primary FROM cms_sites s JOIN cms_settings c ON c.site_id=s.id WHERE s.id=$1")
        .bind(site_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
    let context = SiteContext {
        site,
        base: String::new(),
        canonical_origin: state.origin.as_ref().clone(),
        template: None,
    };
    let html = match page {
        "home" => render_home_context(state, context).await?,
        "about" => render_about_context(state, context).await?,
        "blog" => render_blog_context(state, context).await?,
        _ => return Err(ApiError::invalid("Page d’aperçu inconnue.")),
    };
    preview_document(&html, page, area)
}
fn preview_document(html: &str, page: &str, area: &str) -> ApiResult<String> {
    let (head, rest) = html.split_once("</head>").ok_or_else(ApiError::internal)?;
    // The preview is private and can exist before a domain is connected. Do
    // not advertise a public canonical URL for this authenticated document.
    let head = if let Some((before, canonical)) = head.split_once("<link rel=\"canonical\"") {
        let (_, after) = canonical.split_once('>').ok_or_else(ApiError::internal)?;
        format!("{before}{after}")
    } else {
        head.to_string()
    };
    let (_, body) = rest.split_once("<body>").ok_or_else(ApiError::internal)?;
    let (body, _) = body.rsplit_once("</body>").ok_or_else(ApiError::internal)?;
    // These delimiters are authored by document(); all submitted strings are
    // escaped before they reach it, so client text cannot introduce a fragment.
    let fragment = match area {
        "full" => body,
        "header" => {
            let start = body
                .find("<header class=\"site-header\"")
                .ok_or_else(ApiError::internal)?;
            let end = body[start..]
                .find("</header>")
                .ok_or_else(ApiError::internal)?
                + start
                + "</header>".len();
            &body[start..end]
        }
        "footer" => {
            let start = body
                .find("<footer class=\"site-footer\"")
                .ok_or_else(ApiError::internal)?;
            let end = body[start..]
                .find("</footer>")
                .ok_or_else(ApiError::internal)?
                + start
                + "</footer>".len();
            &body[start..end]
        }
        _ => return Err(ApiError::invalid("Zone d’aperçu inconnue.")),
    };
    // Preview links have the real public targets but cannot navigate this frame.
    let fragment = fragment.replace("<a ", "<a tabindex=\"-1\" aria-disabled=\"true\" ");
    Ok(format!("{head}<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'self'; img-src 'self' https: data:; object-src 'none'; base-uri 'none'; form-action 'none'\"><meta name=\"robots\" content=\"noindex,nofollow\"><link rel=\"stylesheet\" href=\"/assets/template-preview.css\"></head><body inert class=\"template-preview preview-{area}\" data-template-preview=\"true\" data-preview-page=\"{page}\" data-preview-area=\"{area}\">{fragment}</body></html>"))
}

async fn render_content(
    state: &AppState,
    headers: &HeaderMap,
    site_slug: Option<&str>,
    slug: &str,
    kind: &str,
) -> ApiResult<String> {
    let mut context = resolve_site(state, headers, site_slug).await?;
    context.template = builder::published_template(state, context.site.id).await?;
    let site = &context.site;
    let value:serde_json::Value=sqlx::query_scalar("SELECT c.published_snapshot FROM cms_contents c JOIN cms_settings s ON s.site_id=c.site_id WHERE c.site_id=$1 AND c.published_slug=$2 AND c.kind=$3 AND c.published_snapshot IS NOT NULL AND (c.kind='page' OR s.blog_enabled)")
        .bind(site.id).bind(slug).bind(kind).fetch_optional(&state.pool).await?.ok_or_else(ApiError::missing)?;
    let item: Published = serde_json::from_value(value).map_err(|_| ApiError::internal())?;
    let items = contents(state, site.id).await?;
    Ok(content_document(
        &context,
        &item,
        &items,
        &content_path(&context.base, &item),
    ))
}
fn content_document(
    context: &SiteContext,
    item: &Published,
    items: &[Published],
    path: &str,
) -> String {
    let site = &context.site;
    let base = &context.base;
    if let Some(layout) = &item.layout {
        return document(
            context,
            path,
            &format!("{} — {}", item.title, site.site_name),
            &item.excerpt,
            items,
            &render_sections(layout, base),
        );
    }
    let mut body=format!("<article><header class=\"article-header\"><a class=\"back-link\" href=\"{}\">← Retour à l'accueil</a><h1>{}</h1><p class=\"article-excerpt\">{}</p>",escape(home_path(base)),escape(&item.title),escape(&item.excerpt));
    if item.kind == "article" {
        body.push_str(&format!(
            "<p class=\"card-meta\">Publié le <time datetime=\"{}\">{}</time></p>",
            escape(&item.published_at.to_rfc3339()),
            item.published_at.format("%d/%m/%Y")
        ));
    }
    body.push_str("</header><div class=\"article-body\">");
    for paragraph in item.body.split("\n\n").filter(|value| !value.is_empty()) {
        body.push_str(&format!("<p>{}</p>", escape(paragraph)));
    }
    body.push_str("</div></article>");
    let title = format!("{} — {}", item.title, site.site_name);
    let description = if item.excerpt.is_empty() {
        site.description.as_str()
    } else {
        item.excerpt.as_str()
    };
    document(context, path, &title, description, items, &body)
}

fn respond(result: ApiResult<String>) -> Response {
    match result {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            let (title, message) = if error.0 == StatusCode::NOT_FOUND {
                (
                    "Page introuvable",
                    "Cette page n'existe pas ou n'est plus publiée.",
                )
            } else {
                (
                    "Page indisponible",
                    "Le site est temporairement indisponible. Réessayez dans quelques instants.",
                )
            };
            (error.0,Html(format!("<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><title>{title}</title><link rel=\"stylesheet\" href=\"/assets/public.css\"></head><body><main><section class=\"error-panel\"><h1>{title}</h1><p>{message}</p><a href=\"/\">Retour à l'accueil</a></section></main></body></html>"))).into_response()
        }
    }
}
pub async fn home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let host = match RequestHost::from_headers(&headers) {
        Ok(host) => host,
        Err(error) => return respond(Err(error)),
    };
    if !configured_host(&state, &host) {
        return respond(render_home(&state, &headers, None).await);
    }
    let initialized: Result<bool, sqlx::Error> =
        sqlx::query_scalar("SELECT completed FROM cms_setup WHERE singleton=true")
            .fetch_one(&state.pool)
            .await;
    match initialized {
        Ok(true) => respond(render_home(&state, &headers, None).await),
        Ok(false) => Html(r##"<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="robots" content="noindex"><title>AwningCMS — Bienvenue</title><meta name="description" content="Votre espace de création de sites vitrines."><link rel="stylesheet" href="/assets/public.css"></head><body><a class="skip-link" href="#main">Aller au contenu</a><header class="site-header"><a class="site-brand" href="/"><span class="brand-mark" aria-hidden="true">A</span><span>AwningCMS</span></a><nav aria-label="Navigation principale"><a class="admin-link" href="/admin">Administration ↗</a></nav></header><main id="main"><section class="hero"><p class="eyebrow">Votre espace de création</p><h1>Bienvenue sur AwningCMS.</h1><p class="lead">Créez vos sites vitrines, publiez leurs pages et activez leurs modules depuis un même espace.</p></section><section class="empty-site"><h2>Votre espace est prêt à être activé</h2><p>Créez votre compte administrateur avec le code d'activation pour commencer.</p><p><a href="/admin">Ouvrir l'administration →</a></p></section></main><footer class="site-footer"><span>AwningCMS</span><a href="/admin">Administration</a></footer></body></html>"##).into_response(),
        Err(error) => respond(Err(ApiError::from(error))),
    }
}
pub async fn site_home(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    respond(render_home(&state, &headers, Some(&slug)).await)
}
pub async fn about(State(state): State<AppState>, headers: HeaderMap) -> Response {
    respond(render_about(&state, &headers, None).await)
}
pub async fn blog(State(state): State<AppState>, headers: HeaderMap) -> Response {
    respond(render_blog(&state, &headers, None).await)
}
pub async fn site_about(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    respond(render_about(&state, &headers, Some(&slug)).await)
}
pub async fn site_blog(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    respond(render_blog(&state, &headers, Some(&slug)).await)
}
pub async fn page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    respond(render_content(&state, &headers, None, &slug, "page").await)
}
pub async fn article(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    respond(render_content(&state, &headers, None, &slug, "article").await)
}
pub async fn site_page(
    State(state): State<AppState>,
    Path((site_slug, slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    respond(render_content(&state, &headers, Some(&site_slug), &slug, "page").await)
}
pub async fn site_article(
    State(state): State<AppState>,
    Path((site_slug, slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    respond(render_content(&state, &headers, Some(&site_slug), &slug, "article").await)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_styles_are_finite_classes_with_responsive_overrides() {
        let style: SectionStyle = serde_json::from_value(serde_json::json!({
            "background":"purple","text_color":"white","font_family":"serif","font_weight":"bold",
            "border_radius":16,"border_width":2,"border_color":"forest",
            "desktop":{"title_size":64,"text_size":18,"padding":{"top":24,"right":32,"bottom":48,"left":32},"margin":{"top":0,"right":0,"bottom":16,"left":0},"content_width":960},
            "tablet":{"title_size":40,"content_width":640},
            "mobile":{"title_size":32,"text_size":16,"align":"right"},"hidden_desktop":true,"hidden_mobile":true
        })).unwrap();
        let classes = section_style_classes(Some(&style));
        for class in [
            "bs-bg-purple",
            "bs-text-white",
            "bs-font-serif",
            "bs-weight-bold",
            "bs-radius-16",
            "bs-border-2",
            "bs-border-color-forest",
            "bs-d-title-64",
            "bs-d-text-18",
            "bs-d-pt-24",
            "bs-d-pr-32",
            "bs-d-pb-48",
            "bs-d-mt-0",
            "bs-d-width-960",
            "bs-t-title-40",
            "bs-t-width-640",
            "bs-m-title-32",
            "bs-m-text-16",
            "bs-m-align-right",
            "bs-hide-desktop",
            "bs-hide-mobile",
        ] {
            assert!(
                classes.split_whitespace().any(|value| value == class),
                "missing {class}"
            );
        }
        assert!(!classes.contains("bs-hide-tablet"));
        assert!(section_style_classes(None).is_empty());
        assert!(section_style_classes(Some(&SectionStyle::default())).is_empty());
        let mut invalid = SectionStyle::default();
        invalid.desktop.title_size = Some(17);
        invalid.border_radius = Some(u16::MAX);
        assert!(section_style_classes(Some(&invalid)).is_empty());
    }

    #[test]
    fn standalone_widgets_escape_content_and_never_emit_inline_styles() {
        let common = serde_json::json!({"id":Uuid::new_v4(),"align":"right","spacing":"small","theme":"paper"});
        let mut sections = Vec::new();
        for (kind, fields) in [
            (
                "heading",
                serde_json::json!({"title":"Titre <em>libre</em>","style":{"background":"rose","font_family":"serif","desktop":{"title_size":48},"mobile":{"title_size":28}}}),
            ),
            (
                "button",
                serde_json::json!({"button":{"label":"Votre <compte>","href":"/mon-compte"}}),
            ),
            ("divider", serde_json::json!({})),
            ("spacer", serde_json::json!({"height":128})),
        ] {
            let mut section = common.clone();
            section["id"] = serde_json::json!(Uuid::new_v4());
            section["type"] = serde_json::json!(kind);
            for (key, value) in fields.as_object().unwrap() {
                section[key] = value.clone();
            }
            sections.push(section);
        }
        let layout: PageDocument =
            serde_json::from_value(serde_json::json!({"version":1,"sections":sections})).unwrap();
        let html = render_sections(&layout, "");
        assert!(html.contains("Titre &lt;em&gt;libre&lt;/em&gt;"));
        assert!(html.contains("Votre &lt;compte&gt;"));
        assert!(html.contains("data-edit-field=\"button.label\""));
        assert!(html.contains("block-widget-button"));
        assert!(html.contains("<hr class=\"block-divider-line\">"));
        assert!(html.contains("block-spacer-height-128"));
        assert!(html.contains("bs-d-title-48 bs-m-title-28"));
        assert_eq!(html.matches("data-section-id=").count(), 4);
        assert!(!html.contains(" style="));
        assert!(!html.contains("<style"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn every_allowed_style_value_has_a_static_stylesheet_rule() {
        let css = include_str!("../static/assets/blocks.css");
        for breakpoint in ["d", "t", "m"] {
            for property in ["title", "text"] {
                for size in builder::FONT_SIZES {
                    assert!(css.contains(&format!(".bs-{breakpoint}-{property}-{size} ")));
                }
            }
            for property in ["pt", "pr", "pb", "pl", "mt", "mr", "mb", "ml"] {
                for size in builder::EDGE_SPACING {
                    assert!(css.contains(&format!(".bs-{breakpoint}-{property}-{size}{{")));
                }
            }
            for width in builder::CONTENT_WIDTHS {
                assert!(css.contains(&format!(".bs-{breakpoint}-width-{width}>")));
            }
        }
        for size in builder::BORDER_RADII {
            assert!(css.contains(&format!(".bs-radius-{size}{{")));
        }
        for size in builder::BORDER_WIDTHS {
            assert!(css.contains(&format!(".bs-border-{size}{{")));
        }
        for size in builder::SPACER_HEIGHTS {
            assert!(css.contains(&format!(".block-spacer-height-{size}{{")));
        }
        assert!(css.contains("@media(min-width:1001px){.site-block.bs-hide-desktop{display:none}}"));
        assert!(css.contains("@media(min-width:721px) and (max-width:1000px){.site-block.bs-hide-tablet{display:none}}"));
        assert!(css.contains("@media(max-width:720px){.site-block.bs-hide-mobile{display:none}}"));
    }

    #[test]
    fn structured_blocks_escape_text_preserve_order_and_drop_unsafe_urls() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let layout: PageDocument = serde_json::from_value(serde_json::json!({
            "version": 1,
            "sections": [
                {"id":first,"type":"hero","align":"left","spacing":"large","theme":"paper","eyebrow":"<script>alert(1)</script>","title":"Une histoire & vous","text":"Texte <img src=x onerror=alert(1)>","image_url":"javascript:alert(1)","image_alt":"Une image","button":{"label":"Ouvrir","href":"//evil.example/path"}},
                {"id":second,"type":"text","align":"center","spacing":"small","theme":"forest","title":"La suite","text":"Une ligne\n\nUn autre paragraphe"}
            ]
        })).unwrap();
        let html = render_sections(&layout, "");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("Une histoire &amp; vous"));
        assert!(html.contains("Texte &lt;img src=x onerror=alert(1)&gt;"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("src=\"javascript:"));
        assert!(!html.contains("href=\"//"));
        assert!(html.contains("<h1 class=\"block-title\" data-edit-field=\"title\">"));
        assert!(html.contains("<h2 class=\"block-title\" data-edit-field=\"title\">La suite</h2>"));
        assert!(html.contains("block-align-center block-space-small block-theme-forest"));
        assert!(html.find(&first.to_string()).unwrap() < html.find(&second.to_string()).unwrap());
        assert!(html.contains("<p>Une ligne</p><p>Un autre paragraphe</p>"));
    }

    #[test]
    fn builder_buttons_respect_named_site_paths_and_safe_external_links() {
        let internal = Button {
            label: "Mon compte".into(),
            href: "/mon-compte".into(),
        };
        assert!(block_button(&internal, "/s/client").contains("href=\"/s/client/mon-compte\""));
        let external = Button {
            label: "En savoir plus".into(),
            href: "https://example.com/path?one=1&two=2".into(),
        };
        assert!(block_button(&external, "/s/client")
            .contains("href=\"https://example.com/path?one=1&amp;two=2\""));
        for href in [
            "javascript:alert(1)",
            "//example.com",
            "data:text/html,test",
            "/\\evil.example",
            "/%2fexample.com",
        ] {
            assert!(
                block_button(
                    &Button {
                        label: "Ouvrir".into(),
                        href: href.into()
                    },
                    ""
                )
                .is_empty(),
                "{href}"
            );
        }
    }

    #[test]
    fn four_column_sections_have_a_four_column_layout() {
        let layout: PageDocument = serde_json::from_value(serde_json::json!({"version":1,"sections":[{"type":"columns","id":Uuid::new_v4(),"align":"left","spacing":"medium","theme":"paper","title":"Nos services","items":[{"title":"Un","text":"A"},{"title":"Deux","text":"B"},{"title":"Trois","text":"C"},{"title":"Quatre","text":"D"}]}]})).unwrap();
        let html = render_sections(&layout, "");
        assert!(html.contains("block-columns-four"));
        assert_eq!(html.matches("class=\"block-column\"").count(), 4);
    }

    #[test]
    fn template_links_use_published_slugs_and_skip_unpublished_pages() {
        let published_id = Uuid::new_v4();
        let items = vec![Published {
            id: published_id,
            kind: "page".into(),
            title: "Publié".into(),
            slug: "ancien-slug".into(),
            excerpt: String::new(),
            body: String::new(),
            layout: None,
            published_at: Utc::now(),
        }];
        let links: Vec<MenuLink> = serde_json::from_value(serde_json::json!([
            {"id":Uuid::new_v4(),"label":"Page du menu","target":{"kind":"page","id":published_id}},
            {"id":Uuid::new_v4(),"label":"Brouillon privé","target":{"kind":"page","id":Uuid::new_v4()}},
            {"id":Uuid::new_v4(),"label":"Mon compte","target":{"kind":"builtin","page":"account"}}
        ])).unwrap();
        let navigation = menu_links("", &items, &links);
        assert_eq!(navigation.len(), 2);
        assert_eq!(navigation[0].path, "/pages/ancien-slug");
        assert_eq!(navigation[0].label, "Page du menu");
        assert_eq!(navigation[1].path, "/mon-compte");
        assert_eq!(navigation[1].icon, Some("user-round"));
    }

    #[test]
    fn previews_keep_safe_styles_and_disable_active_content() {
        let source = "<!doctype html><html><head><link rel=\"canonical\" href=\"https://example.com/\"><link rel=\"stylesheet\" href=\"/assets/blocks.css\"></head><body><header class=\"site-header\" data-template-area=\"header\"><a href=\"/\">Accueil</a></header><main>Test</main><footer class=\"site-footer\" data-template-area=\"footer\">Pied</footer></body></html>";
        let html = preview_document(source, "builder", "full").unwrap();
        assert!(!html.contains("rel=\"canonical\""));
        assert!(html.contains("<body inert"));
        assert!(html.contains("Content-Security-Policy"));
        assert!(html.contains("script-src 'none'"));
        assert!(html.contains("form-action 'none'"));
        assert!(html.contains("aria-disabled=\"true\""));
        assert!(preview_document(source, "home", "header")
            .unwrap()
            .contains("data-template-area=\"header\""));
        assert!(!preview_document(source, "home", "header")
            .unwrap()
            .contains("<main>"));
        assert!(preview_document(source, "home", "footer")
            .unwrap()
            .contains("data-template-area=\"footer\""));
    }

    #[test]
    fn visitor_account_reuses_published_template_shell_and_keeps_private_form_mount() {
        let template: TemplateDocument = serde_json::from_value(serde_json::json!({"version":1,"header":{"model":"centered","links":[{"id":Uuid::new_v4(),"label":"Notre accueil","target":{"kind":"builtin","page":"home"}}]},"footer":{"model":"columns","description":"Notre pied de page <protégé>","links":[{"id":Uuid::new_v4(),"label":"Espace personnel","target":{"kind":"builtin","page":"account"}}]}})).unwrap();
        let context = SiteContext {
            site: Site {
                id: Uuid::new_v4(),
                slug: "client".into(),
                site_name: "Mon client".into(),
                description: String::new(),
                is_primary: false,
            },
            base: String::new(),
            canonical_origin: "https://client.example".into(),
            template: Some(template),
        };
        let html = account_document(&context, &[]);
        assert!(html.contains("data-template-area=\"header\" data-model=\"centered\""));
        assert!(html.contains("data-template-area=\"footer\" data-model=\"columns\""));
        assert!(html.contains("Notre accueil"));
        assert!(html.contains("Notre pied de page &lt;protégé&gt;"));
        assert!(html.contains("id=\"account-app\""));
        assert!(html.contains("/assets/account.js\" defer"));
        assert!(html.contains("name=\"robots\" content=\"noindex, nofollow\""));
        assert_eq!(html.matches("<main ").count(), 1);
        assert_eq!(html.matches("id=\"main\"").count(), 1);
        assert!(!html.contains("class=\"account-header\""));
    }

    #[test]
    fn escapes_text_and_attributes_without_interpreting_html() {
        assert_eq!(
            escape("<script>alert(\"x\")</script>&'"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&amp;&#39;"
        );
        assert_eq!(escape("L'été à Cassagnères"), "L&#39;été à Cassagnères");
    }
    #[test]
    fn extranet_shell_stays_on_the_client_domain_without_caching() {
        let response = extranet_shell(Uuid::nil());
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key(header::LOCATION));
        assert_eq!(
            response.headers()["x-cms-extranet"],
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    #[test]
    fn canonical_urls_keep_the_public_domain_separate_from_the_admin_domain() {
        assert_eq!(
            canonical_url("https://www.example.com", "/pages/contact"),
            "https://www.example.com/pages/contact"
        );
        assert_eq!(
            canonical_url("https://cms.example.com", "/s/client/pages/contact"),
            "https://cms.example.com/s/client/pages/contact"
        );
    }

    #[test]
    fn request_hosts_match_exact_origins_and_ports() {
        let host = RequestHost::parse("WWW.Example.COM:443").unwrap();
        assert_eq!(host.hostname, "www.example.com");
        assert!(host.matches_origin("https://www.example.com"));
        assert!(!host.matches_origin("https://cms.example.com"));
        assert!(!host.matches_origin("https://www.example.com.evil.example"));
        assert!(RequestHost::parse("127.0.0.1:18080")
            .unwrap()
            .matches_origin("http://127.0.0.1:18080"));
        assert!(!RequestHost::parse("127.0.0.1")
            .unwrap()
            .matches_origin("http://127.0.0.1:18080"));
        assert!(RequestHost::parse("[::1]:8080")
            .unwrap()
            .matches_origin("http://[::1]:8080"));
        assert!(!RequestHost::parse("www.example.com:8443")
            .unwrap()
            .matches_origin("https://www.example.com"));
    }

    #[test]
    fn request_hosts_reject_ambiguous_authorities_and_ignore_forwarded_host() {
        for value in [
            "",
            " cms.example.com",
            "cms.example.com ",
            "cms.example.com/path",
            "cms.example.com\\evil",
            "cms.example.com.",
            "cms.example.com@evil.example",
            "cms.example.com?evil",
            "cms.example.com#evil",
            "cms.example.com%2eevil.example",
            "cms.example.com,evil.example",
            "cms.example.com:",
            "cms.example.com:0",
            "cms.example.com:65536",
            "cms.example.com:443:80",
            "cássagneres.com",
            "[::1]evil",
            "::1",
        ] {
            assert!(RequestHost::parse(value).is_err(), "accepted {value:?}");
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("cms.example.com"),
        );
        assert!(RequestHost::from_headers(&headers).is_err());
        headers.insert(header::HOST, HeaderValue::from_static("unknown.example"));
        assert_eq!(
            RequestHost::from_headers(&headers).unwrap().hostname,
            "unknown.example"
        );
        headers.append(header::HOST, HeaderValue::from_static("cms.example.com"));
        assert!(RequestHost::from_headers(&headers).is_err());
    }

    #[test]
    fn primary_www_origin_allows_only_its_exact_https_apex_alias() {
        for host in ["example.com", "EXAMPLE.COM:443"] {
            assert_eq!(
                primary_apex_origin(
                    "https://www.example.com",
                    &RequestHost::parse(host).unwrap()
                ),
                Some("https://example.com".into())
            );
        }
        for (origin, host) in [
            ("https://www.example.com", "example.com:8443"),
            ("https://www.example.com", "example.com:80"),
            ("https://www.example.com", "other.example.com"),
            ("https://www.example.com", "example.com.evil.example"),
            ("https://www.example.com:8443", "example.com"),
            ("http://www.example.com", "example.com"),
            ("https://cms.example.com", "example.com"),
        ] {
            assert_eq!(
                primary_apex_origin(origin, &RequestHost::parse(host).unwrap()),
                None
            );
        }
    }

    #[test]
    fn bound_site_document_uses_its_own_canonical_and_root_links() {
        let context = SiteContext {
            site: Site {
                id: Uuid::nil(),
                slug: "client".into(),
                site_name: "Le client".into(),
                description: "Sa vitrine".into(),
                is_primary: false,
            },
            base: String::new(),
            canonical_origin: "https://www.client.example".into(),
            template: None,
        };
        let item = Published {
            id: Uuid::nil(),
            kind: "page".into(),
            title: "Contact".into(),
            slug: "contact".into(),
            excerpt: String::new(),
            body: String::new(),
            layout: None,
            published_at: Utc::now(),
        };
        let html = document(
            &context,
            "/pages/contact",
            "Contact",
            "Sa vitrine",
            &[item],
            "Contact",
        );
        assert!(
            html.contains("rel=\"canonical\" href=\"https://www.client.example/pages/contact\"")
        );
        assert!(html.contains("href=\"/pages/contact\""));
        assert!(html.contains("href=\"/mon-compte\""));
        assert!(html.contains("href=\"/a-propos\""));
        assert!(html.contains("href=\"/blog\""));
        assert!(html.contains("aria-label=\"Mon compte\""));
        assert!(html.contains("/assets/icons/user-round.svg"));
        assert!(!html.contains(">Extranet<"));
        assert!(!html.contains("/s/client"));
    }
}
