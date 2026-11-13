//! Validated, versioned visual documents. The public renderer reads only the
//! published snapshots; saving a draft never changes a public site.
use crate::{
    error::{ApiError, ApiResult},
    public, structure, validation, AppState,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Align {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Spacing {
    Small,
    #[default]
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Theme {
    #[default]
    Paper,
    Sage,
    Forest,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundColor {
    #[default]
    Default,
    White,
    Cream,
    Sage,
    Forest,
    Ink,
    Purple,
    Blue,
    Rose,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TextColor {
    #[default]
    Default,
    Ink,
    Muted,
    White,
    Forest,
    Purple,
    Blue,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FontFamily {
    #[default]
    Inherit,
    Sans,
    Serif,
    Mono,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FontWeight {
    Normal,
    Medium,
    Semibold,
    Bold,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct EdgeValues {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct DeviceStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_size: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<EdgeValues>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<EdgeValues>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SectionStyle {
    pub background: BackgroundColor,
    pub text_color: TextColor,
    pub font_family: FontFamily,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<FontWeight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<u16>,
    pub border_color: TextColor,
    pub desktop: DeviceStyle,
    pub tablet: DeviceStyle,
    pub mobile: DeviceStyle,
    pub hidden_desktop: bool,
    pub hidden_tablet: bool,
    pub hidden_mobile: bool,
}
pub(crate) const FONT_SIZES: &[u16] = &[12, 14, 16, 18, 20, 24, 28, 32, 40, 48, 56, 64, 80, 96];
pub(crate) const EDGE_SPACING: &[u16] = &[0, 4, 8, 12, 16, 24, 32, 48, 64, 80, 96, 120];
pub(crate) const CONTENT_WIDTHS: &[u16] = &[0, 480, 640, 800, 960, 1120, 1280];
pub(crate) const BORDER_RADII: &[u16] = &[0, 4, 8, 12, 16, 24, 32, 48];
pub(crate) const BORDER_WIDTHS: &[u16] = &[0, 1, 2, 4];
pub(crate) const SPACER_HEIGHTS: &[u16] = &[8, 16, 24, 32, 48, 64, 96, 128, 192, 240];

fn allowed_style_value(value: Option<u16>, options: &[u16], label: &str) -> ApiResult<()> {
    if value.is_some_and(|value| !options.contains(&value)) {
        return Err(ApiError::invalid(format!(
            "{label} doit utiliser une valeur proposée par l’éditeur."
        )));
    }
    Ok(())
}
impl EdgeValues {
    fn validate(&self) -> ApiResult<()> {
        for value in [self.top, self.right, self.bottom, self.left] {
            allowed_style_value(Some(value), EDGE_SPACING, "L’espacement")?;
        }
        Ok(())
    }
}
impl DeviceStyle {
    fn validate(&self) -> ApiResult<()> {
        allowed_style_value(self.title_size, FONT_SIZES, "La taille du titre")?;
        allowed_style_value(self.text_size, FONT_SIZES, "La taille du texte")?;
        allowed_style_value(self.content_width, CONTENT_WIDTHS, "La largeur du contenu")?;
        if let Some(padding) = &self.padding {
            padding.validate()?;
        }
        if let Some(margin) = &self.margin {
            margin.validate()?;
        }
        Ok(())
    }
}
impl SectionStyle {
    pub(crate) fn validate(&self) -> ApiResult<()> {
        allowed_style_value(self.border_radius, BORDER_RADII, "L’arrondi")?;
        allowed_style_value(self.border_width, BORDER_WIDTHS, "L’épaisseur de bordure")?;
        self.desktop.validate()?;
        self.tablet.validate()?;
        self.mobile.validate()?;
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Button {
    pub label: String,
    pub href: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Column {
    pub title: String,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Section {
    Hero {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        eyebrow: String,
        title: String,
        text: String,
        image_url: Option<String>,
        image_alt: String,
        button: Option<Button>,
    },
    Text {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        title: String,
        text: String,
    },
    Image {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        url: String,
        alt: String,
        caption: String,
    },
    Columns {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        title: String,
        items: Vec<Column>,
    },
    Cta {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        title: String,
        text: String,
        button: Button,
    },
    Heading {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        title: String,
    },
    Button {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        button: Button,
    },
    Divider {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
    },
    Spacer {
        id: Uuid,
        align: Align,
        spacing: Spacing,
        theme: Theme,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<SectionStyle>,
        height: u16,
    },
}
impl Section {
    pub(crate) fn id(&self) -> Uuid {
        match self {
            Self::Hero { id, .. }
            | Self::Text { id, .. }
            | Self::Image { id, .. }
            | Self::Columns { id, .. }
            | Self::Cta { id, .. }
            | Self::Heading { id, .. }
            | Self::Button { id, .. }
            | Self::Divider { id, .. }
            | Self::Spacer { id, .. } => *id,
        }
    }
    pub(crate) fn style(&self) -> Option<&SectionStyle> {
        match self {
            Self::Hero { style, .. }
            | Self::Text { style, .. }
            | Self::Image { style, .. }
            | Self::Columns { style, .. }
            | Self::Cta { style, .. }
            | Self::Heading { style, .. }
            | Self::Button { style, .. }
            | Self::Divider { style, .. }
            | Self::Spacer { style, .. } => style.as_ref(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageDocument {
    pub version: u32,
    pub sections: Vec<Section>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HeaderModel {
    Classic,
    Centered,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterModel {
    Simple,
    Columns,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BuiltinPage {
    Home,
    About,
    Blog,
    Account,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MenuTarget {
    Builtin { page: BuiltinPage },
    Page { id: Uuid },
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct MenuLink {
    pub id: Uuid,
    pub label: String,
    pub target: MenuTarget,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub model: HeaderModel,
    pub links: Vec<MenuLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Footer {
    pub model: FooterModel,
    pub description: String,
    pub links: Vec<MenuLink>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateDocument {
    pub version: u32,
    pub header: Header,
    pub footer: Footer,
}

fn bound(value: &str, label: &str, max: usize) -> ApiResult<()> {
    validation::bounded(value, label, 0, max)
}
fn text_required(value: &str, label: &str, max: usize) -> ApiResult<()> {
    validation::bounded(value.trim(), label, 1, max)
}
fn external_https(value: &str) -> ApiResult<url::Url> {
    if value.len() > 2048
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains('\\')
    {
        return Err(ApiError::invalid("L’adresse du lien est invalide."));
    }
    let url = url::Url::parse(value)
        .map_err(|_| ApiError::invalid("Utilisez une adresse HTTPS complète."))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
        || url.port().is_some_and(|p| p != 443)
    {
        return Err(ApiError::invalid(
            "Utilisez une adresse HTTPS publique, sans identifiants ni port personnalisé.",
        ));
    }
    Ok(url)
}
pub(crate) fn validate_href(value: &str) -> ApiResult<()> {
    if value.starts_with('/')
        && !value.starts_with("//")
        && !["%2f", "%5c", "%0a", "%0d"]
            .iter()
            .any(|part| value.to_ascii_lowercase().contains(part))
        && !value.contains('\\')
        && value.len() <= 2048
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Ok(());
    }
    external_https(value).map(|_| ())
}
pub(crate) fn validate_image(value: &str) -> ApiResult<()> {
    if value.starts_with("/assets/")
        && value.len() <= 300
        && value.is_ascii()
        && !value.contains(['%', '?', '#', '\\'])
        && !value.split('/').any(|part| part == ".." || part == ".")
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Ok(());
    }
    let url = external_https(value)?;
    let public = match url.host() {
        Some(url::Host::Domain(host)) => {
            host.contains('.')
                && ![".localhost", ".local", ".internal", ".test", ".invalid"]
                    .iter()
                    .any(|s| host.ends_with(s))
        }
        Some(url::Host::Ipv4(ip)) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
        }
        Some(url::Host::Ipv6(ip)) => {
            ip.to_ipv4_mapped().is_none()
                && !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && !ip.is_multicast()
        }
        None => false,
    };
    if !public {
        return Err(ApiError::invalid(
            "Choisissez une image disponible sur une adresse HTTPS publique.",
        ));
    }
    // No server-side request is made for any supplied image URL.
    Ok(())
}
fn validate_button(button: &Button) -> ApiResult<()> {
    text_required(&button.label, "Le libellé du bouton", 80)?;
    validate_href(&button.href)
}
impl PageDocument {
    pub(crate) fn validate(&self) -> ApiResult<()> {
        if self.version != 1 || self.sections.len() > 50 {
            return Err(ApiError::invalid(
                "Le document doit utiliser la version 1 et contenir au maximum 50 sections.",
            ));
        }
        let mut ids = HashSet::new();
        for section in &self.sections {
            if section.id().is_nil() || !ids.insert(section.id()) {
                return Err(ApiError::invalid(
                    "Chaque section doit posséder un identifiant distinct.",
                ));
            }
            if let Some(style) = section.style() {
                style.validate()?;
            }
            match section {
                Section::Heading { title, .. } => text_required(title, "Le titre", 200)?,
                Section::Button { button, .. } => validate_button(button)?,
                Section::Divider { .. } => {}
                Section::Spacer { height, .. } => {
                    allowed_style_value(Some(*height), SPACER_HEIGHTS, "La hauteur de l’espace")?
                }
                Section::Hero {
                    eyebrow,
                    title,
                    text,
                    image_url,
                    image_alt,
                    button,
                    ..
                } => {
                    bound(eyebrow, "Le surtitre", 100)?;
                    text_required(title, "Le titre", 200)?;
                    bound(text, "Le texte", 5000)?;
                    bound(image_alt, "La description de l’image", 300)?;
                    if let Some(image) = image_url {
                        validate_image(image)?;
                    }
                    if let Some(button) = button {
                        validate_button(button)?;
                    }
                }
                Section::Text { title, text, .. } => {
                    bound(title, "Le titre", 200)?;
                    bound(text, "Le texte", 100000)?;
                }
                Section::Image {
                    url, alt, caption, ..
                } => {
                    validate_image(url)?;
                    bound(alt, "La description de l’image", 300)?;
                    bound(caption, "La légende", 500)?;
                }
                Section::Columns { title, items, .. } => {
                    bound(title, "Le titre", 200)?;
                    if !(2..=4).contains(&items.len()) {
                        return Err(ApiError::invalid(
                            "Une section de colonnes doit contenir de 2 à 4 colonnes.",
                        ));
                    }
                    for item in items {
                        bound(&item.title, "Le titre de colonne", 200)?;
                        bound(&item.text, "Le texte de colonne", 5000)?;
                    }
                }
                Section::Cta {
                    title,
                    text,
                    button,
                    ..
                } => {
                    text_required(title, "Le titre", 200)?;
                    bound(text, "Le texte", 5000)?;
                    validate_button(button)?;
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| ApiError::internal())?
            .len()
            > 450_000
        {
            return Err(ApiError::invalid(
                "Le contenu de cette page est trop volumineux.",
            ));
        }
        Ok(())
    }
}
impl TemplateDocument {
    pub(crate) fn validate(&self) -> ApiResult<()> {
        if self.version != 1 || self.header.links.len() > 10 || self.footer.links.len() > 20 {
            return Err(ApiError::invalid("Le modèle doit utiliser la version 1, avec 10 liens d’en-tête et 20 liens de pied de page au maximum."));
        }
        bound(
            &self.footer.description,
            "La description du pied de page",
            1000,
        )?;
        let mut ids = HashSet::new();
        for link in self.header.links.iter().chain(&self.footer.links) {
            if link.id.is_nil() || !ids.insert(link.id) {
                return Err(ApiError::invalid(
                    "Chaque lien doit posséder un identifiant distinct.",
                ));
            }
            text_required(&link.label, "Le libellé du lien", 80)?;
            if matches!(link.target,MenuTarget::Page{id} if id.is_nil()) {
                return Err(ApiError::invalid("La page du lien est invalide."));
            }
        }
        Ok(())
    }
    async fn validate_pages(
        &self,
        connection: &mut PgConnection,
        site: Uuid,
        publishing: bool,
    ) -> ApiResult<()> {
        self.validate()?;
        for link in self.header.links.iter().chain(&self.footer.links) {
            if let MenuTarget::Page { id } = link.target {
                let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_contents WHERE site_id=$1 AND id=$2 AND kind='page' AND (NOT $3 OR published_snapshot IS NOT NULL))")
                    .bind(site).bind(id).bind(publishing).fetch_one(&mut *connection).await?;
                if !found {
                    return Err(ApiError::invalid(if publishing {
                        "Publiez d’abord les pages choisies dans les menus."
                    } else {
                        "Un lien du menu ne désigne pas une page de ce site."
                    }));
                }
            }
        }
        Ok(())
    }
}
fn page_from_value(value: Value) -> ApiResult<PageDocument> {
    let doc: PageDocument = serde_json::from_value(value)
        .map_err(|_| ApiError::invalid("Le format des sections est invalide."))?;
    doc.validate()?;
    Ok(doc)
}
fn template_from_value(value: Value) -> ApiResult<TemplateDocument> {
    let doc: TemplateDocument = serde_json::from_value(value)
        .map_err(|_| ApiError::invalid("Le format du modèle est invalide."))?;
    doc.validate()?;
    Ok(doc)
}
fn as_value<T: Serialize>(value: &T) -> ApiResult<Value> {
    serde_json::to_value(value).map_err(|_| ApiError::internal())
}
fn target(value: &str) -> ApiResult<()> {
    if matches!(value, "home" | "template") {
        Ok(())
    } else {
        Err(ApiError::missing())
    }
}
async fn lock_site(connection: &mut PgConnection, id: Uuid) -> ApiResult<()> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM cms_sites WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(())
}
fn revision(expected: i64, current: i64) -> ApiResult<()> {
    if expected < 0 {
        return Err(ApiError::invalid("La révision est invalide."));
    }
    if expected != current {
        return Err(ApiError::conflict(
            "Ce document a été modifié dans une autre fenêtre. Rechargez-le avant d’enregistrer.",
        ));
    }
    Ok(())
}

pub(crate) fn default_template() -> TemplateDocument {
    let links = |offset: u128| {
        [
            ("Accueil", BuiltinPage::Home),
            ("À propos", BuiltinPage::About),
            ("Blog", BuiltinPage::Blog),
            ("Mon compte", BuiltinPage::Account),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (label, page))| MenuLink {
            id: Uuid::from_u128(offset + i as u128),
            label: label.into(),
            target: MenuTarget::Builtin { page },
        })
        .collect()
    };
    TemplateDocument {
        version: 1,
        header: Header {
            model: HeaderModel::Classic,
            links: links(100),
        },
        footer: Footer {
            model: FooterModel::Simple,
            description: String::new(),
            links: links(200),
        },
    }
}
fn default_home(name: &str, description: &str) -> PageDocument {
    PageDocument {
        version: 1,
        sections: vec![Section::Hero {
            id: Uuid::from_u128(1),
            align: Align::Left,
            spacing: Spacing::Large,
            theme: Theme::Paper,
            style: None,
            eyebrow: "BIENVENUE".into(),
            title: name.into(),
            text: if description.is_empty() {
                "Notre site est en cours de construction. Nous préparons un nouvel espace pour vous accueillir et partager nos actualités.".into()
            } else {
                description.into()
            },
            image_url: Some("/assets/site-construction.webp".into()),
            image_alt: "Illustration du site en préparation".into(),
            button: Some(Button {
                label: "À propos".into(),
                href: "/a-propos".into(),
            }),
        }],
    }
}
fn legacy_page(title: &str, excerpt: &str, body: &str) -> PageDocument {
    PageDocument {
        version: 1,
        sections: vec![
            Section::Hero {
                id: Uuid::from_u128(1),
                align: Align::Left,
                spacing: Spacing::Medium,
                theme: Theme::Paper,
                style: None,
                eyebrow: String::new(),
                title: title.into(),
                text: excerpt.into(),
                image_url: None,
                image_alt: String::new(),
                button: None,
            },
            Section::Text {
                id: Uuid::from_u128(2),
                align: Align::Left,
                spacing: Spacing::Medium,
                theme: Theme::Paper,
                style: None,
                title: String::new(),
                text: body.into(),
            },
        ],
    }
}
#[derive(Serialize)]
pub struct DocumentResponse {
    pub(crate) document: Value,
    pub(crate) revision: i64,
    pub(crate) published_revision: Option<i64>,
    pub(crate) published_at: Option<DateTime<Utc>>,
    pub(crate) has_unpublished_changes: bool,
}
#[derive(FromRow)]
struct DocumentRow {
    draft: Value,
    revision: i64,
    published_snapshot: Option<Value>,
    published_revision: Option<i64>,
    published_at: Option<DateTime<Utc>>,
}
impl DocumentRow {
    fn response(self) -> DocumentResponse {
        DocumentResponse {
            has_unpublished_changes: self.published_snapshot.as_ref() != Some(&self.draft),
            document: self.draft,
            revision: self.revision,
            published_revision: self.published_revision,
            published_at: self.published_at,
        }
    }
}
async fn read_design(state: &AppState, site: Uuid, kind: &str) -> ApiResult<DocumentResponse> {
    target(kind)?;
    let (name, description): (String, String) =
        sqlx::query_as("SELECT site_name,description FROM cms_settings WHERE site_id=$1")
            .bind(site)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(ApiError::missing)?;
    let row:Option<DocumentRow>=sqlx::query_as("SELECT draft,revision,published_snapshot,published_revision,published_at FROM cms_design_documents WHERE site_id=$1 AND target=$2").bind(site).bind(kind).fetch_optional(&state.pool).await?;
    if let Some(row) = row {
        return Ok(row.response());
    }
    let doc = if kind == "home" {
        as_value(&default_home(&name, &description))?
    } else {
        as_value(&default_template())?
    };
    Ok(DocumentResponse {
        document: doc,
        revision: 0,
        published_revision: None,
        published_at: None,
        has_unpublished_changes: false,
    })
}
pub async fn design_read(
    State(state): State<AppState>,
    Path((site, kind)): Path<(Uuid, String)>,
) -> ApiResult<Json<DocumentResponse>> {
    Ok(Json(read_design(&state, site, &kind).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInput {
    expected_revision: i64,
    document: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishInput {
    expected_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    document: Value,
}
pub async fn design_save(
    State(state): State<AppState>,
    Path((site, kind)): Path<(Uuid, String)>,
    Json(input): Json<SaveInput>,
) -> ApiResult<Json<DocumentResponse>> {
    Ok(Json(
        save_design(&state, site, &kind, input.expected_revision, input.document).await?,
    ))
}
async fn save_design(
    state: &AppState,
    site: Uuid,
    kind: &str,
    expected: i64,
    document: Value,
) -> ApiResult<DocumentResponse> {
    target(kind)?;
    let doc = if kind == "home" {
        as_value(&page_from_value(document)?)?
    } else {
        as_value(&template_from_value(document)?)?
    };
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    if kind == "template" {
        template_from_value(doc.clone())?
            .validate_pages(&mut tx, site, false)
            .await?;
    }
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM cms_design_documents WHERE site_id=$1 AND target=$2",
    )
    .bind(site)
    .bind(kind)
    .fetch_optional(&mut *tx)
    .await?;
    revision(expected, current.unwrap_or(0))?;
    let row:DocumentRow=sqlx::query_as("INSERT INTO cms_design_documents(site_id,target,draft,revision) VALUES($1,$2,$3,1) ON CONFLICT(site_id,target) DO UPDATE SET draft=excluded.draft,revision=cms_design_documents.revision+1,updated_at=now() RETURNING draft,revision,published_snapshot,published_revision,published_at")
        .bind(site).bind(kind).bind(doc).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row.response())
}
pub async fn design_publish(
    State(state): State<AppState>,
    Path((site, kind)): Path<(Uuid, String)>,
    Json(input): Json<PublishInput>,
) -> ApiResult<Json<DocumentResponse>> {
    Ok(Json(
        publish_design(&state, site, &kind, input.expected_revision).await?,
    ))
}
async fn publish_design(
    state: &AppState,
    site: Uuid,
    kind: &str,
    expected: i64,
) -> ApiResult<DocumentResponse> {
    target(kind)?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    let existing:DocumentRow=sqlx::query_as("SELECT draft,revision,published_snapshot,published_revision,published_at FROM cms_design_documents WHERE site_id=$1 AND target=$2").bind(site).bind(kind).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::conflict("Enregistrez le brouillon avant de le publier."))?;
    revision(expected, existing.revision)?;
    if kind == "template" {
        template_from_value(existing.draft.clone())?
            .validate_pages(&mut tx, site, true)
            .await?;
    } else {
        page_from_value(existing.draft.clone())?;
    }
    let row:DocumentRow=sqlx::query_as("UPDATE cms_design_documents SET published_snapshot=draft,published_revision=revision,published_at=now(),updated_at=now() WHERE site_id=$1 AND target=$2 RETURNING draft,revision,published_snapshot,published_revision,published_at").bind(site).bind(kind).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row.response())
}
fn preview_response(html: String) -> Response {
    let mut response = Html(html).into_response();
    response.extensions_mut().insert(structure::PreviewResponse);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
}
pub async fn design_preview(
    State(state): State<AppState>,
    Path((site, kind)): Path<(Uuid, String)>,
    Json(input): Json<PreviewInput>,
) -> ApiResult<Response> {
    target(&kind)?;
    let html = if kind == "home" {
        let page = page_from_value(input.document)?;
        public::render_builder_preview(&state, site, Some(&page), None, None).await?
    } else {
        let template = template_from_value(input.document)?;
        let mut connection = state.pool.acquire().await?;
        template
            .validate_pages(&mut connection, site, false)
            .await?;
        drop(connection);
        public::render_builder_preview(&state, site, None, Some(&template), None).await?
    };
    Ok(preview_response(html))
}
#[derive(FromRow)]
struct LayoutRow {
    title: String,
    slug: String,
    excerpt: String,
    body: String,
    layout_draft: Option<Value>,
    revision: i64,
    published_snapshot: Option<Value>,
    published_at: Option<DateTime<Utc>>,
}
impl LayoutRow {
    fn response(self) -> ApiResult<DocumentResponse> {
        let metadata_changed = self.published_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.get("title").and_then(Value::as_str) != Some(self.title.as_str())
                || snapshot.get("slug").and_then(Value::as_str) != Some(self.slug.as_str())
                || snapshot.get("excerpt").and_then(Value::as_str) != Some(self.excerpt.as_str())
                || snapshot.get("body").and_then(Value::as_str) != Some(self.body.as_str())
        });
        let document = match self.layout_draft {
            Some(value) => as_value(&page_from_value(value)?)?,
            None => as_value(&legacy_page(&self.title, &self.excerpt, &self.body))?,
        };
        let published_revision = self
            .published_snapshot
            .as_ref()
            .and_then(|s| s.get("revision").and_then(Value::as_i64));
        let published_doc = self.published_snapshot.as_ref().map(|s| {
            s.get("layout")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::to_value(legacy_page(
                        s.get("title").and_then(Value::as_str).unwrap_or(""),
                        s.get("excerpt").and_then(Value::as_str).unwrap_or(""),
                        s.get("body").and_then(Value::as_str).unwrap_or(""),
                    ))
                    .expect("serializable document")
                })
        });
        Ok(DocumentResponse {
            has_unpublished_changes: metadata_changed || published_doc.as_ref() != Some(&document),
            document,
            revision: self.revision,
            published_revision,
            published_at: self.published_at,
        })
    }
}
async fn read_layout(connection: &mut PgConnection, site: Uuid, id: Uuid) -> ApiResult<LayoutRow> {
    sqlx::query_as("SELECT title,slug,excerpt,body,layout_draft,revision,published_snapshot,published_at FROM cms_contents WHERE site_id=$1 AND id=$2 AND kind='page'").bind(site).bind(id).fetch_optional(connection).await?.ok_or_else(ApiError::missing)
}
pub async fn layout_read(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<DocumentResponse>> {
    Ok(Json(read_page_draft(&state, site, id).await?))
}
pub(crate) async fn read_page_draft(
    state: &AppState,
    site: Uuid,
    id: Uuid,
) -> ApiResult<DocumentResponse> {
    let mut connection = state.pool.acquire().await?;
    read_layout(&mut connection, site, id).await?.response()
}
pub async fn layout_save(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
    Json(input): Json<SaveInput>,
) -> ApiResult<Json<DocumentResponse>> {
    let doc = page_from_value(input.document)?;
    Ok(Json(
        save_page_draft(&state, site, id, input.expected_revision, doc).await?,
    ))
}
pub(crate) async fn save_page_draft(
    state: &AppState,
    site: Uuid,
    id: Uuid,
    expected: i64,
    doc: PageDocument,
) -> ApiResult<DocumentResponse> {
    doc.validate()?;
    let document = as_value(&doc)?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    let existing = read_layout(&mut tx, site, id).await?;
    revision(expected, existing.revision)?;
    sqlx::query("UPDATE cms_contents SET layout_draft=$3,revision=revision+1,updated_at=now() WHERE site_id=$1 AND id=$2 AND kind='page'").bind(site).bind(id).bind(document).execute(&mut *tx).await?;
    let row = read_layout(&mut tx, site, id).await?;
    tx.commit().await?;
    row.response()
}
pub async fn layout_preview(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
    Json(input): Json<PreviewInput>,
) -> ApiResult<Response> {
    let document = page_from_value(input.document)?;
    let mut connection = state.pool.acquire().await?;
    read_layout(&mut connection, site, id).await?;
    drop(connection);
    Ok(preview_response(
        public::render_builder_preview(&state, site, Some(&document), None, Some(id)).await?,
    ))
}
pub(crate) async fn published_home(
    state: &AppState,
    site: Uuid,
) -> ApiResult<Option<PageDocument>> {
    let value:Option<Value>=sqlx::query_scalar("SELECT published_snapshot FROM cms_design_documents WHERE site_id=$1 AND target='home' AND published_snapshot IS NOT NULL").bind(site).fetch_optional(&state.pool).await?;
    value.map(page_from_value).transpose()
}
pub(crate) async fn published_template(
    state: &AppState,
    site: Uuid,
) -> ApiResult<Option<TemplateDocument>> {
    let value:Option<Value>=sqlx::query_scalar("SELECT published_snapshot FROM cms_design_documents WHERE site_id=$1 AND target='template' AND published_snapshot IS NOT NULL").bind(site).fetch_optional(&state.pool).await?;
    value.map(template_from_value).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn widget(kind: &str) -> Value {
        serde_json::json!({"id":Uuid::new_v4(),"type":kind,"align":"right","spacing":"small","theme":"paper"})
    }
    fn one_widget(value: Value) -> ApiResult<PageDocument> {
        page_from_value(serde_json::json!({"version":1,"sections":[value]}))
    }
    #[test]
    fn old_documents_keep_their_original_json_and_typography() {
        let original = as_value(&default_home("Site existant", "Présentation")).unwrap();
        assert!(original["sections"][0].get("style").is_none());
        let parsed = page_from_value(original.clone()).unwrap();
        assert_eq!(as_value(&parsed).unwrap(), original);
        assert!(parsed.sections[0].style().is_none());
        let mut with_style = original["sections"][0].clone();
        with_style["style"] = serde_json::json!({"background":"white"});
        let parsed = one_widget(with_style).unwrap();
        let style = parsed.sections[0].style().unwrap();
        assert_eq!(style.background, BackgroundColor::White);
        assert_eq!(style.font_weight, None);
        assert_eq!(style.font_family, FontFamily::Inherit);
        assert_eq!(style.desktop, DeviceStyle::default());
        assert!(!style.hidden_mobile);
    }
    #[test]
    fn new_widgets_and_device_styles_round_trip_as_typed_documents() {
        let mut heading = widget("heading");
        heading["title"] = "Un titre personnalisable".into();
        heading["style"] = serde_json::json!({
            "background":"purple","text_color":"white","font_family":"serif","font_weight":"semibold",
            "border_radius":24,"border_width":2,"border_color":"blue",
            "desktop":{"title_size":80,"text_size":20,"padding":{"top":80,"right":24,"bottom":64,"left":24},"margin":{"top":0,"right":0,"bottom":16,"left":0},"content_width":1120,"align":"right"},
            "tablet":{"title_size":48,"content_width":640,"align":"center"},
            "mobile":{"title_size":32,"text_size":16,"padding":{"top":24,"right":16,"bottom":24,"left":16},"content_width":0,"align":"left"},
            "hidden_desktop":false,"hidden_tablet":true,"hidden_mobile":false
        });
        let mut button = widget("button");
        button["button"] = serde_json::json!({"label":"Mon compte","href":"/mon-compte"});
        let divider = widget("divider");
        let mut spacer = widget("spacer");
        spacer["height"] = 192.into();
        let page = page_from_value(
            serde_json::json!({"version":1,"sections":[heading,button,divider,spacer]}),
        )
        .unwrap();
        assert_eq!(page.sections.len(), 4);
        assert!(matches!(
            &page.sections[0],
            Section::Heading {
                align: Align::Right,
                ..
            }
        ));
        assert_eq!(
            page.sections[0].style().unwrap().mobile.content_width,
            Some(0)
        );
        assert!(page.sections[0].style().unwrap().hidden_tablet);
        assert_eq!(page_from_value(as_value(&page).unwrap()).unwrap(), page);
    }
    #[test]
    fn styles_reject_raw_css_unknown_fields_and_unlisted_values() {
        let mut heading = widget("heading");
        heading["title"] = "Titre".into();
        for bad in [
            serde_json::json!({"css":"position:fixed"}),
            serde_json::json!({"background":"url(https://example.com/x)"}),
            serde_json::json!({"text_color":"#ff0000"}),
            serde_json::json!({"font_family":"Arial; color:red"}),
            serde_json::json!({"font_weight":900}),
            serde_json::json!({"border_radius":9}),
            serde_json::json!({"border_width":3}),
            serde_json::json!({"desktop":{"title_size":19}}),
            serde_json::json!({"tablet":{"text_size":0}}),
            serde_json::json!({"mobile":{"content_width":500}}),
            serde_json::json!({"mobile":{"padding":{"top":-4,"right":0,"bottom":0,"left":0}}}),
            serde_json::json!({"desktop":{"margin":{"top":7,"right":0,"bottom":0,"left":0}}}),
            serde_json::json!({"mobile":{"padding":{"top":4}}}),
            serde_json::json!({"mobile":{"position":"fixed"}}),
            serde_json::json!({"hidden_mobile":"yes"}),
        ] {
            heading["style"] = bad;
            assert!(
                one_widget(heading.clone()).is_err(),
                "{:?}",
                heading["style"]
            );
        }
    }
    #[test]
    fn new_widgets_preserve_content_and_size_constraints() {
        let mut heading = widget("heading");
        for invalid in [String::new(), " ".into(), "x".repeat(201)] {
            heading["title"] = invalid.into();
            assert!(one_widget(heading.clone()).is_err());
        }
        let mut button = widget("button");
        button["button"] = serde_json::json!({"label":"Ouvrir","href":"javascript:alert(1)"});
        assert!(one_widget(button).is_err());
        let mut divider = widget("divider");
        divider["html"] = "<hr onclick=bad()>".into();
        assert!(one_widget(divider).is_err());
        let mut spacer = widget("spacer");
        for height in SPACER_HEIGHTS {
            spacer["height"] = (*height).into();
            assert!(one_widget(spacer.clone()).is_ok());
        }
        for height in [0, 4, 80, 120, 241, 10000] {
            spacer["height"] = height.into();
            assert!(one_widget(spacer.clone()).is_err());
        }
    }
    #[test]
    fn documents_are_typed_bounded_and_have_unique_sections() {
        let mut doc = default_home("Un site", "");
        assert!(doc.validate().is_ok());
        doc.sections.push(doc.sections[0].clone());
        assert!(doc.validate().is_err());
        let mut value = as_value(&default_home("Un site", "")).unwrap();
        value["sections"][0]["html"] = Value::String("<script>bad()</script>".into());
        assert!(page_from_value(value).is_err());
        let mut doc = default_home("Un site", "");
        doc.version = 2;
        assert!(doc.validate().is_err());
        doc.version = 1;
        doc.sections = vec![doc.sections[0].clone(); 51];
        assert!(doc.validate().is_err());
    }
    #[test]
    fn legacy_content_and_long_site_names_are_preserved() {
        let text = "😀".repeat(100000);
        let page = legacy_page("Titre", "Résumé", &text);
        assert!(page.validate().is_ok());
        assert!(matches!(&page.sections[1],Section::Text{text:body,..} if body==&text));
        assert!(default_home(&"a".repeat(200), &"b".repeat(500))
            .validate()
            .is_ok());
    }
    #[test]
    fn unsafe_links_and_private_images_are_rejected_without_fetching() {
        for link in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "//example.com",
            "/\\evil.com",
            "/%2f%2fevil.com",
            "https://name:pass@example.com/a",
            "https://example.com:8000/a",
        ] {
            assert!(validate_href(link).is_err(), "{link}");
        }
        for link in [
            "/a-propos",
            "/pages/contact#message",
            "https://example.com/page",
        ] {
            assert!(validate_href(link).is_ok());
        }
        for image in [
            "https://127.0.0.1/a.png",
            "https://10.1.2.3/a.png",
            "https://[::1]/a.png",
            "https://[::ffff:127.0.0.1]/a.png",
            "https://host.local/a.png",
            "/assets/../secret",
            "/assets/%2e%2e/x",
        ] {
            assert!(validate_image(image).is_err(), "{image}");
        }
        assert!(validate_image("/assets/site-construction.webp").is_ok());
        assert!(validate_image("https://images.example.com/photo.webp").is_ok());
    }
    #[test]
    fn templates_reject_unknown_models_and_duplicate_links() {
        let mut doc = default_template();
        assert!(doc.validate().is_ok());
        doc.footer.links[0].id = doc.header.links[0].id;
        assert!(doc.validate().is_err());
        let mut value = as_value(&default_template()).unwrap();
        value["header"]["model"] = Value::String("custom-html".into());
        assert!(template_from_value(value).is_err());
        assert!(revision(2, 2).is_ok());
        assert!(revision(1, 2).is_err());
    }
}
