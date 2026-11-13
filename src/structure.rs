//! Read-only inventory of every site page and authenticated previews of the
//! shared public template. Client scope is enforced by the extranet router.
use crate::{
    error::{ApiError, ApiResult},
    public, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Serialize)]
pub struct Structure {
    site_id: Uuid,
    pages: Vec<Page>,
    template: Template,
}
#[derive(Serialize)]
struct Page {
    id: String,
    title: String,
    path: String,
    published_path: Option<String>,
    kind: &'static str,
    status: &'static str,
    description: String,
    content_id: Option<Uuid>,
    has_unpublished_changes: bool,
    updated_at: Option<DateTime<Utc>>,
}
#[derive(Serialize)]
struct Template {
    id: &'static str,
    name: &'static str,
    header: Navigation,
    footer: Navigation,
}
#[derive(Serialize)]
struct Navigation {
    links: Vec<public::NavigationLink>,
}
#[derive(FromRow)]
struct PageContent {
    id: Uuid,
    title: String,
    slug: String,
    published_slug: Option<String>,
    published_title: Option<String>,
    has_unpublished_changes: bool,
    updated_at: DateTime<Utc>,
}
#[derive(FromRow)]
struct HomeDesign {
    published: bool,
    changed: bool,
    updated_at: DateTime<Utc>,
}
fn page_path(slug: &str) -> String {
    if slug == "a-propos" {
        "/a-propos".into()
    } else {
        format!("/pages/{slug}")
    }
}
fn system_page(id: &str, title: &str, path: &str, status: &'static str, description: &str) -> Page {
    Page {
        id: id.into(),
        title: title.into(),
        path: path.into(),
        published_path: Some(path.into()),
        kind: "system",
        status,
        description: description.into(),
        content_id: None,
        has_unpublished_changes: false,
        updated_at: None,
    }
}
fn page_inventory(
    blog_enabled: bool,
    has_visible_content: bool,
    rows: &[PageContent],
) -> Vec<Page> {
    let about = rows
        .iter()
        .find(|item| item.published_slug.as_deref() == Some("a-propos"))
        .or_else(|| rows.iter().find(|item| item.slug == "a-propos"));
    let mut about_page = system_page(
        "about",
        "À propos",
        "/a-propos",
        "automatic",
        "Présentation d’attente du modèle. Vous pouvez la personnaliser avec une page À propos.",
    );
    if let Some(item) = about {
        about_page.content_id = Some(item.id);
        about_page.updated_at = Some(item.updated_at);
        about_page.has_unpublished_changes = item.has_unpublished_changes;
        if item.published_slug.as_deref() == Some("a-propos") {
            about_page.status = "published";
            about_page.description = if item.has_unpublished_changes {
                "Votre présentation publiée reste visible. Des modifications sont en brouillon."
                    .into()
            } else {
                "Votre présentation personnalisée est publiée sur le site.".into()
            };
        } else {
            about_page.description = "La présentation du modèle reste visible. Votre page personnalisée est en préparation.".into();
        }
    }
    let mut pages = vec![
        system_page(
            "home",
            "Accueil",
            "/",
            "automatic",
            if has_visible_content {
                "Page d’accueil du modèle avec les contenus publiés du site."
            } else {
                "Bienvenue et message de construction en attendant vos premières publications."
            },
        ),
        about_page,
        system_page(
            "blog",
            "Blog",
            "/blog",
            if blog_enabled {
                "automatic"
            } else {
                "disabled"
            },
            if blog_enabled {
                "Liste automatique des articles publiés, ou message d’attente si le blog est vide."
            } else {
                "Page d’attente disponible. Activez le module Blog pour afficher vos articles."
            },
        ),
        system_page(
            "account",
            "Mon compte",
            "/mon-compte",
            "private",
            "Inscription et connexion des visiteurs à leur espace personnel, séparé de l’équipe du site.",
        ),
    ];
    for item in rows {
        let belongs_to_about = about.is_some_and(|about| about.id == item.id);
        let published_alias = belongs_to_about
            && item
                .published_slug
                .as_deref()
                .is_some_and(|slug| slug != "a-propos");
        if belongs_to_about && !published_alias {
            continue;
        }
        // A draft renamed to a-propos can still have a separate live URL. Keep
        // that live page in the inventory until the draft is published.
        let path = match item.published_slug.as_deref() {
            Some(slug) if published_alias => page_path(slug),
            _ => page_path(&item.slug),
        };
        pages.push(Page {
            id: format!("content:{}", item.id),
            title: if published_alias { item.published_title.as_ref().unwrap_or(&item.title).clone() } else { item.title.clone() },
            path,
            published_path: item.published_slug.as_deref().map(page_path),
            kind: "content",
            status: if item.published_slug.is_some() { "published" } else { "draft" },
            description: if published_alias {
                "Version publiée conservée à cette adresse. Le brouillon est lié à la page À propos.".into()
            } else if item.has_unpublished_changes {
                "Version publiée visible ; modifications en brouillon non publiées.".into()
            } else if item.published_slug.is_some() {
                "Page personnalisée publiée sur le site.".into()
            } else {
                "Brouillon privé, non visible par les visiteurs.".into()
            },
            content_id: Some(item.id), has_unpublished_changes: item.has_unpublished_changes,
            updated_at: Some(item.updated_at),
        });
    }
    pages
}

pub async fn read(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
) -> ApiResult<Json<Structure>> {
    let blog_enabled: bool =
        sqlx::query_scalar("SELECT blog_enabled FROM cms_settings WHERE site_id=$1")
            .bind(site_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(ApiError::missing)?;
    let rows: Vec<PageContent> = sqlx::query_as("SELECT id,title,slug,published_slug,published_snapshot->>'title' AS published_title, published_snapshot IS NOT NULL AND (published_snapshot->>'title' IS DISTINCT FROM title OR published_snapshot->>'slug' IS DISTINCT FROM slug OR published_snapshot->>'excerpt' IS DISTINCT FROM excerpt OR published_snapshot->>'body' IS DISTINCT FROM body OR COALESCE(published_snapshot->'layout','null'::jsonb) IS DISTINCT FROM COALESCE(layout_draft,'null'::jsonb)) AS has_unpublished_changes,updated_at FROM cms_contents WHERE site_id=$1 AND kind='page' ORDER BY updated_at DESC,id")
        .bind(site_id).fetch_all(&state.pool).await?;
    let has_visible_content: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_contents WHERE site_id=$1 AND published_snapshot IS NOT NULL AND (kind='page' OR $2))")
        .bind(site_id).bind(blog_enabled).fetch_one(&state.pool).await?;
    let (header, footer) = public::template_navigation(&state, site_id).await?;
    let mut pages = page_inventory(blog_enabled, has_visible_content, &rows);
    let home_design: Option<HomeDesign> = sqlx::query_as("SELECT published_snapshot IS NOT NULL AS published, draft IS DISTINCT FROM published_snapshot AS changed, updated_at FROM cms_design_documents WHERE site_id=$1 AND target='home'")
        .bind(site_id).fetch_optional(&state.pool).await?;
    if let Some(home) = home_design {
        let page = &mut pages[0];
        page.has_unpublished_changes = home.changed;
        page.updated_at = Some(home.updated_at);
        page.status = if home.published {
            "published"
        } else {
            "automatic"
        };
        page.description = match (home.published, home.changed) {
            (true, true) => {
                "Accueil personnalisé publié. Des modifications visuelles sont en brouillon."
            }
            (true, false) => "Accueil personnalisé publié depuis le constructeur visuel.",
            (false, _) => {
                "L’accueil du modèle reste visible. Votre composition visuelle est en brouillon."
            }
        }
        .into();
    }
    let custom_template: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_design_documents WHERE site_id=$1 AND target='template' AND published_snapshot IS NOT NULL)")
        .bind(site_id).fetch_one(&state.pool).await?;
    Ok(Json(Structure {
        site_id,
        pages,
        template: Template {
            id: if custom_template {
                "custom-v1"
            } else {
                "welcome-v1"
            },
            name: if custom_template {
                "Personnalisé"
            } else {
                "Essentiel"
            },
            header: Navigation { links: header },
            footer: Navigation { links: footer },
        },
    }))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewPage {
    #[default]
    Home,
    About,
    Blog,
}
impl PreviewPage {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::About => "about",
            Self::Blog => "blog",
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewArea {
    #[default]
    Full,
    Header,
    Footer,
}
impl PreviewArea {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Header => "header",
            Self::Footer => "footer",
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewQuery {
    page: PreviewPage,
    area: PreviewArea,
}
// Only the authenticated HTML handler can set this internal response marker.
// It is not a request-controlled path, query parameter or HTTP header.
#[derive(Clone, Copy)]
pub(crate) struct PreviewResponse;
pub async fn preview(
    State(state): State<AppState>,
    Path(site_id): Path<Uuid>,
    Query(query): Query<PreviewQuery>,
) -> ApiResult<Response> {
    let html =
        public::render_template_preview(&state, site_id, query.page.as_str(), query.area.as_str())
            .await?;
    let mut response = Html(html).into_response();
    response.extensions_mut().insert(PreviewResponse);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(slug: &str, published_slug: Option<&str>) -> PageContent {
        PageContent {
            id: Uuid::new_v4(),
            title: "Titre du brouillon".into(),
            slug: slug.into(),
            published_slug: published_slug.map(str::to_owned),
            published_title: published_slug.map(|_| "Titre publié".into()),
            has_unpublished_changes: published_slug.is_some_and(|value| value != slug),
            updated_at: Utc::now(),
        }
    }
    #[test]
    fn inventory_keeps_default_pages_and_private_drafts() {
        let rows = [item("contact", None)];
        let pages = page_inventory(false, false, &rows);
        assert_eq!(pages.len(), 5);
        assert_eq!(pages[2].status, "disabled");
        assert_eq!(pages[3].status, "private");
        assert_eq!(pages[4].status, "draft");
        assert!(pages[4].published_path.is_none());
    }
    #[test]
    fn published_about_is_folded_into_builtin_even_after_draft_rename() {
        let rows = [item("notre-equipe", Some("a-propos"))];
        let pages = page_inventory(true, true, &rows);
        assert_eq!(pages.len(), 4);
        assert_eq!(pages[1].content_id, Some(rows[0].id));
        assert_eq!(pages[1].status, "published");
        assert!(pages[1].has_unpublished_changes);
        assert_eq!(pages[1].published_path.as_deref(), Some("/a-propos"));
    }
    #[test]
    fn renaming_to_about_keeps_old_live_page_in_inventory() {
        let rows = [item("a-propos", Some("notre-equipe"))];
        let pages = page_inventory(false, true, &rows);
        assert_eq!(pages.len(), 5);
        assert_eq!(pages[1].status, "automatic");
        assert_eq!(pages[4].title, "Titre publié");
        assert_eq!(
            pages[4].published_path.as_deref(),
            Some("/pages/notre-equipe")
        );
    }
}
