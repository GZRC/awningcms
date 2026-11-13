'use strict';

(() => {
  const app = document.getElementById('app');
  const url = new URL(window.location.href);
  const paths = window.location.pathname.split('/').filter(Boolean);
  const isExtranet = paths[0] === 'extranet';
  const adminBase = isExtranet ? '/extranet' : '/admin';
  const requestedSites = url.searchParams.getAll('site');
  const requestedSite = !isExtranet && requestedSites.length === 1 && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(requestedSites[0]) ? requestedSites[0].toLowerCase() : null;
  const state = { user: null, sites: [], site: null, settings: null, main: null, dirty: false };
  const icons = {
    home: 'M3 10 12 3l9 7M5 9v12h14V9M9 21v-7h6v7',
    sites: 'M3 4h18v14H3zM3 8h18M8 21h8M12 18v3',
    page: 'M14 2H5v20h14V7zM14 2v6h5M8 12h8M8 16h8',
    article: 'M4 3h16v18H4zM8 7h8M8 11h8M8 15h5',
    modules: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
    settings: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M9 3l-1 3-3 1-2 3 2 2-1 3 2 3 3 1 1 3h4l1-3 3-1 2-3-1-3 2-2-2-3-3-1-1-3z',
    arrow: 'M5 12h14M13 6l6 6-6 6',
    external: 'M14 3h7v7M21 3 10 14M10 3H3v18h18v-7',
    plus: 'M12 5v14M5 12h14',
    search: 'M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14M15 15l6 6',
    menu: 'M3 6h18M3 12h18M3 18h18',
    logout: 'M9 3H3v18h6M9 12h12M16 7l5 5-5 5',
    check: 'M5 12l4 4L19 6',
    edit: 'm15 4 5 5M3 21l5-1L21 7l-5-5L3 15z',
    clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M12 7v5l3 2',
    back: 'M19 12H5M11 6l-6 6 6 6',
    lock: 'M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4M12 14v3',
  };
  function h(tag, attrs = {}, ...children) {
    const element = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) {
      if (value === null || value === undefined || value === false) continue;
      if (key === 'class') element.className = value;
      else if (key === 'text') element.textContent = value;
      else if (key.startsWith('on')) element.addEventListener(key.slice(2).toLowerCase(), value);
      else if (key === 'value' || key === 'checked' || key === 'disabled') element[key] = value;
      else element.setAttribute(key, value === true ? '' : key === 'href' ? workspacePath(String(value)) : String(value));
    }
    for (const child of children.flat(Infinity)) {
      if (child !== null && child !== undefined && child !== false) element.append(child instanceof Node ? child : document.createTextNode(String(child)));
    }
    return element;
  }
  function icon(name) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'icon'); svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', icons[name] || icons.page); svg.append(path); return svg;
  }
  function workspacePath(path) {
    if (!isExtranet || !/^\/admin(?:[/?#]|$)/.test(path)) return path;
    const target = new URL(path, url.origin);
    target.pathname = target.pathname.replace(/^\/admin/, '/extranet');
    target.searchParams.delete('site');
    return target.pathname + target.search + target.hash;
  }
  function link(label, href, classes = '', symbol) { return h('a', { href, class: classes }, symbol ? icon(symbol) : null, label); }
  function button(label, classes = 'primary', symbol, onClick) {
    return h('button', { type: 'button', class: `button ${classes}`, onClick }, symbol ? icon(symbol) : null, label);
  }
  function adminLink(view = '', site = state.site) {
    const query = new URLSearchParams();
    if (site && !isExtranet) query.set('site', site.id);
    if (view) query.set('view', view);
    return adminBase + (query.size ? `?${query}` : '');
  }
  const roleLabels = { admin: 'Administrateur', editor: 'Éditeur', viewer: 'Lecture seule', owner_support: 'Session d’assistance' };
  const roleDescriptions = { admin: 'Gère les contenus, les réglages et les membres de l’équipe.', editor: 'Crée, modifie et publie les pages et les articles.', viewer: 'Consulte les contenus et les réglages, sans les modifier.' };
  function canManageTeam() { return !isExtranet || ['admin', 'owner_support'].includes(state.user?.role); }
  function canEditContent() { return canManageTeam() || state.user?.role === 'editor'; }
  function readOnlyNotice(administration = false) { return h('p', { class: 'notice role-notice' }, 'Votre rôle permet de consulter cet espace. Les modifications sont réservées aux ', administration ? 'administrateurs du site.' : 'administrateurs et aux éditeurs.'); }
  function siteQuery() { return state.site && !isExtranet ? `?site=${encodeURIComponent(state.site.id)}` : ''; }
  // Authentication returns only to this CMS and carries a validated site id,
  // never an arbitrary return URL or the other incoming query parameters.
  function authLink(path) {
    const siteId = state.site?.id || requestedSite;
    return isExtranet ? '/extranet' : path + (siteId ? `?site=${encodeURIComponent(siteId)}` : '');
  }
  function newContentLink(kind) { return workspacePath(`/admin/contents/new?kind=${kind}&site=${encodeURIComponent(state.site.id)}`); }
  function builderLink(target) { return `${adminLink('builder')}&target=${encodeURIComponent(target)}`; }
  function contentEditorLink(content) { return workspacePath(`/admin/contents/${encodeURIComponent(content.id)}?site=${encodeURIComponent(state.site.id)}&kind=${content.kind}`); }
  function publicBase() { return isExtranet ? '' : `/s/${encodeURIComponent(state.site.slug)}`; }
  function publicPageLink(path) { return `${publicBase()}${path === '/' ? '' : path}` || '/'; }
  function publicLink(content) { const slug = content.published_slug || content.slug; return publicPageLink(content.kind === 'page' && slug === 'a-propos' ? '/a-propos' : `/${content.kind === 'article' ? 'articles' : 'pages'}/${encodeURIComponent(slug)}`); }
  function endpoint(path) { return `/api/sites/${encodeURIComponent(state.site.id)}${path}`; }
  function date(value, long = false) {
    if (!value) return '—';
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return '—';
    return new Intl.DateTimeFormat('fr-FR', long ? { day: 'numeric', month: 'long', year: 'numeric', hour: '2-digit', minute: '2-digit' } : { day: 'numeric', month: 'short', year: 'numeric' }).format(parsed);
  }
  function slugify(value) {
    return value.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 100).replace(/-$/, '');
  }
  class ApiError extends Error { constructor(message, status) { super(message); this.status = status; } }
  async function api(path, options = {}) {
    let response;
    try {
      if (isExtranet && path.startsWith('/api/')) path = '/extranet' + path;
      response = await fetch(path, {
        method: options.method || 'GET', credentials: 'same-origin', cache: 'no-store',
        headers: options.method ? { 'Content-Type': 'application/json', Accept: 'application/json' } : { Accept: 'application/json' },
        ...(options.method ? { body: JSON.stringify(options.body || {}) } : {}),
      });
    } catch { throw new ApiError('Connexion impossible. Vérifiez votre connexion puis réessayez.', 0); }
    let data = null;
    const text = await response.text();
    if (text) { try { data = JSON.parse(text); } catch { throw new ApiError('Le serveur a renvoyé une réponse inattendue. Réessayez dans un instant.', response.status); } }
    if (!response.ok) {
      if (response.status === 401 && state.user && !options.keepDraft) {
        state.dirty = false;
        window.location.assign(authLink('/admin/login'));
      }
      throw new ApiError(data?.error || 'Cette opération n’a pas abouti. Réessayez.', response.status);
    }
    return data;
  }
  function toast(message, error = false) {
    const node = h('div', { class: `toast${error ? ' error' : ''}` }, icon(error ? 'clock' : 'check'), h('span', {}, message));
    document.getElementById('notifications').append(node);
    window.setTimeout(() => node.remove(), error ? 10000 : 5000);
  }
  function heading(title, subtitle, actions = [], eyebrow = 'VOTRE ESPACE DE GESTION') {
    document.title = `${title} · AwningCMS`;
    return h('div', { class: 'page-heading' }, h('div', {}, h('p', { class: 'eyebrow' }, eyebrow), h('h1', {}, title), subtitle ? h('p', { class: 'page-subtitle' }, subtitle) : null), h('div', { class: 'heading-actions' }, actions));
  }
  function empty(title, description, action, name = 'page') {
    return h('div', { class: 'empty-state' }, h('div', { class: 'empty-icon' }, icon(name)), h('h3', {}, title), h('p', {}, description), action);
  }
  function showError(error, retry) {
    state.main?.replaceChildren(h('section', { class: 'error-panel' }, h('h1', {}, 'Impossible de charger cet espace'), h('p', {}, error.message), button('Réessayer', 'primary', null, retry || (() => window.location.reload()))));
    state.main?.removeAttribute('aria-busy');
  }
  function status(content) {
    const changed = content.published && content.has_unpublished_changes;
    return h('span', { class: `status${changed ? ' changed' : content.published ? ' published' : ''}` }, changed ? 'Modifications en brouillon' : content.published ? 'Publié' : 'Brouillon');
  }
  function field(label, name, options = {}) {
    const input = h(options.textarea ? 'textarea' : 'input', { id: name, name, type: options.type || 'text', value: options.value || '', required: options.required, maxlength: options.maxlength, minlength: options.minlength, autocomplete: options.autocomplete, placeholder: options.placeholder, rows: options.rows, pattern: options.pattern, class: options.class, 'aria-describedby': options.hint ? `${name}-hint` : null });
    const node = h('div', { class: 'field' }, h('label', { for: name }, label), input, options.hint ? h('p', { class: 'hint', id: `${name}-hint` }, options.hint) : null);
    return { node, input };
  }
  function setBusy(form, busy) { for (const item of form.elements) item.disabled = busy; form.setAttribute('aria-busy', String(busy)); }
  function formError(message) { return h('div', { class: 'form-error', role: 'alert', hidden: !message }, message); }

  function shell() {
    const activeView = url.searchParams.get('view') || 'dashboard';
    const sidebar = h('aside', { class: 'sidebar', 'aria-label': 'Navigation de l’administration' });
    sidebar.append(h('button', { type: 'button', class: 'sidebar-close', 'aria-label': 'Fermer le menu', onClick: () => { document.documentElement.classList.remove('nav-open'); document.querySelector('.mobile-toggle')?.setAttribute('aria-expanded', 'false'); } }, '×'));
    sidebar.append(link('', '/admin', 'sidebar-brand'));
    sidebar.querySelector('.sidebar-brand').append(h('span', { class: 'brand-mark', 'aria-hidden': 'true' }, 'A'), h('span', {}, h('strong', {}, isExtranet ? state.site?.name || 'Votre site' : 'AwningCMS'), h('small', {}, isExtranet ? 'Votre extranet' : 'L’espace de vos sites')));
    const selector = h('select', { id: 'site-selector', 'aria-label': 'Site client actif', onChange: e => { if (!state.dirty || window.confirm('Quitter sans enregistrer vos modifications ?')) { state.dirty = false; window.location.assign(adminLink('', state.sites.find(s => s.id === e.target.value))); } else e.target.value = state.site.id; } });
    for (const site of state.sites) selector.append(h('option', { value: site.id }, site.name));
    if (state.site) selector.value = state.site.id;
    if (!isExtranet) sidebar.append(h('div', { class: 'site-switcher' }, h('label', { for: 'site-selector' }, 'SITE ACTIF'), selector));
    const navigation = h('nav', {}, h('p', { class: 'nav-label' }, 'GÉRER LE SITE'));
    const entries = [['dashboard', 'Vue d’ensemble', 'home', adminLink()], ['pages', 'Pages', 'page', adminLink('pages')], ['template', 'Modèle du site', 'sites', adminLink('template')], ['articles', 'Articles', 'article', adminLink('articles')], ['modules', 'Modules', 'modules', adminLink('modules')], ['settings', 'Réglages', 'settings', '/admin/settings' + siteQuery()], ['users', 'Utilisateurs', 'lock', adminLink('users')]];
    for (const [view, label, symbol, href] of entries) {
      if (view === 'users' && !canManageTeam()) continue;
      if (view === 'articles' && state.settings && !state.settings.blog_enabled) continue;
      const current = (view === activeView && paths.length === 1) || (view === 'pages' && paths[1] === 'pages') || (view === 'settings' && paths[1] === 'settings') || (paths[1] === 'contents' && (view === 'articles' && url.searchParams.get('kind') === 'article' || view === 'pages' && url.searchParams.get('kind') !== 'article'));
      const nav = link(label, href, 'nav-link' + (current ? ' active' : ''), symbol);
      if (current) nav.setAttribute('aria-current', 'page');
      navigation.append(nav);
    }
    if (!isExtranet) navigation.append(h('p', { class: 'nav-label nav-separator' }, 'VOTRE ACTIVITÉ'), link('Sites clients', '/admin?view=sites', 'nav-link' + (activeView === 'sites' || paths[1] === 'sites' ? ' active' : ''), 'sites'));
    sidebar.append(navigation, h('div', { class: 'nav-spacer' }));
    const logout = h('button', { type: 'button', class: 'logout', title: 'Se déconnecter', 'aria-label': 'Se déconnecter', onClick: async () => {
      if (state.dirty && !window.confirm('Quitter sans enregistrer vos modifications ?')) return;
      logout.disabled = true;
      try { await api('/api/auth/logout', { method: 'POST' }); state.dirty = false; window.location.assign(authLink('/admin/login')); }
      catch (error) { toast(error.message, true); logout.disabled = false; }
    } }, icon('logout'));
    sidebar.append(h('div', { class: 'sidebar-bottom' }, h('div', { class: 'sidebar-note' }, h('span', { class: 'eyebrow' }, 'À VOTRE RYTHME'), h('p', {}, canEditContent() ? 'Créez vos pages, préparez vos contenus et publiez quand tout est prêt.' : 'Consultez les pages de votre site et suivez les dernières publications.')), h('div', { class: 'user-block' }, h('span', { class: 'avatar', 'aria-hidden': 'true' }, (state.user.name || state.user.email).slice(0, 1).toUpperCase()), h('div', { class: 'user-text' }, h('strong', {}, state.user.name), h('span', {}, isExtranet ? roleLabels[state.user.role] || 'Accès client' : 'Administrateur CMS')), logout)));
    const toggle = h('button', { type: 'button', class: 'mobile-toggle', 'aria-label': 'Ouvrir le menu', 'aria-expanded': 'false', onClick: () => { const open = document.documentElement.classList.toggle('nav-open'); toggle.setAttribute('aria-expanded', String(open)); toggle.setAttribute('aria-label', open ? 'Fermer le menu' : 'Ouvrir le menu'); } }, icon('menu'));
    const workspace = h('div', { class: 'workspace' });
    workspace.append(h('header', { class: 'topbar' }, h('div', { class: 'breadcrumb' }, toggle, h('span', {}, isExtranet ? 'Extranet' : 'Espace de travail'), h('span', { 'aria-hidden': 'true' }, '/'), h('strong', {}, state.site?.name || 'Sites clients')), h('div', { class: 'topbar-actions' }, h('span', { class: 'topbar-date' }, new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' }).format(new Date())), state.site ? h('a', { class: 'view-site', href: publicBase() || '/', target: '_blank', rel: 'noopener' }, 'Voir la vitrine', icon('external')) : null)));
    state.main = h('main', { class: 'main-content', id: 'main', 'aria-busy': 'true' }, h('div', { class: 'route-loading' }, 'Chargement…'));
    workspace.append(state.main); app.replaceChildren(sidebar, workspace);
  }

  function contentTable(contents, options = {}) {
    const rows = h('tbody');
    for (const content of contents) {
      const href = contentEditorLink(content);
      rows.append(h('tr', {}, h('td', {}, h('a', { href, class: 'content-title' }, h('span', { class: 'document-icon' }, icon(content.kind === 'article' ? 'article' : 'page')), h('span', {}, h('strong', {}, content.title), h('span', { class: 'content-slug' }, `/${content.slug}`)))), h('td', {}, status(content)), options.mixed ? h('td', { class: 'hide-mobile' }, content.kind === 'article' ? 'Article' : 'Page') : null, h('td', { class: 'hide-mobile' }, date(content.updated_at)), h('td', {}, h('a', { href, class: 'table-edit', 'aria-label': `${canEditContent() ? 'Modifier' : 'Consulter'} ${content.title}` }, icon(canEditContent() ? 'edit' : 'page')))));
    }
    return h('div', { class: 'table-scroll' }, h('table', { class: 'content-table' }, h('thead', {}, h('tr', {}, h('th', { scope: 'col' }, 'Titre'), h('th', { scope: 'col' }, 'Statut'), options.mixed ? h('th', { scope: 'col', class: 'hide-mobile' }, 'Type') : null, h('th', { scope: 'col', class: 'hide-mobile' }, 'Dernière modification'), h('th', { scope: 'col' }, h('span', { class: 'sr-only' }, 'Actions')))), rows));
  }

  async function dashboard() {
    const [numbers, pages, articles, structure] = await Promise.all([api(endpoint('/dashboard')), api(endpoint('/contents?kind=page')), state.settings.blog_enabled ? api(endpoint('/contents?kind=article')) : Promise.resolve([]), api(endpoint('/structure'))]);
    numbers.pages = structure.pages.length;
    const recent = [...pages, ...articles].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at)).slice(0, 6);
    const stats = h('div', { class: 'stats-grid' });
    for (const [label, key, symbol, description] of [['Pages', 'pages', 'page', 'Pages du modèle et pages personnalisées'], ['Articles', 'articles', 'article', state.settings.blog_enabled ? 'Pour partager vos actualités' : 'Module Blog désactivé'], ['Publiés', 'published', 'check', 'Contenus avec une version publiée'], ['Brouillons', 'drafts', 'clock', 'À préparer à votre rythme']]) {
      stats.append(h('section', { class: 'stat-card' }, h('div', { class: 'stat-top' }, h('span', { class: 'stat-label' }, label), icon(symbol)), h('div', { class: 'stat-value' }, numbers[key] ?? 0), h('p', {}, description)));
    }
    const recentPanel = h('section', { class: 'panel' }, h('div', { class: 'panel-header' }, h('div', {}, h('h2', {}, 'Derniers contenus'), h('p', {}, 'Reprenez là où vous en étiez.')), link('Toutes les pages', adminLink('pages'), 'text-link')));
    recentPanel.append(recent.length ? contentTable(recent, { mixed: true }) : empty('Votre première page commence ici', 'Présentez votre activité, vos services ou votre histoire. Les nouveaux contenus restent en brouillon jusqu’à leur publication.', canEditContent() ? link('Créer une page', newContentLink('page'), 'button primary', 'plus') : null));
    const shortcuts = h('section', { class: 'panel workflow-panel' }, h('p', { class: 'eyebrow' }, 'LES PROCHAINES ÉTAPES'), h('h2', {}, 'Faites grandir votre site'), h('p', {}, 'Un espace simple pour construire une vitrine qui vous ressemble.'), h('div', { class: 'quick-links' }, h('a', { class: 'quick-link', href: '/admin/settings' + siteQuery() }, h('span', {}, h('strong', {}, canManageTeam() ? 'Définir votre identité' : 'Identité du site'), h('span', {}, 'Nom et présentation du site')), icon('arrow')), h('a', { class: 'quick-link', href: canEditContent() ? newContentLink('page') : adminLink('pages') }, h('span', {}, h('strong', {}, canEditContent() ? 'Rédiger une page' : 'Consulter les pages'), h('span', {}, 'Les contenus de votre site')), icon('arrow')), h('a', { class: 'quick-link', href: adminLink('modules') }, h('span', {}, h('strong', {}, canManageTeam() ? 'Activer des modules' : 'Consulter les modules'), h('span', {}, canManageTeam() ? 'Ajoutez un blog à votre vitrine' : 'Les fonctions de votre vitrine')), icon('arrow'))));
    state.main.replaceChildren(heading('Vue d’ensemble', `Bienvenue dans l’espace de ${state.site.name}.`, [canEditContent() ? link('Créer une page', newContentLink('page'), 'button primary', 'plus') : null]), stats, h('div', { class: 'dashboard-lower' }, recentPanel, shortcuts));
  }

  async function listContents(kind) {
    if (kind === 'page') return sitePages();
    if (kind === 'article' && !state.settings.blog_enabled) {
      state.main.replaceChildren(heading('Articles', 'Partagez les actualités de votre site.'), h('section', { class: 'panel' }, empty('Le module Blog est désactivé', 'Activez le Blog pour créer des articles et les présenter sur votre vitrine.', link('Découvrir le module Blog', adminLink('modules'), 'button primary', 'modules'), 'article'))); return;
    }
    const contents = await api(endpoint(`/contents?kind=${kind}`));
    const plural = kind === 'article' ? 'articles' : 'pages';
    const singular = kind === 'article' ? 'un article' : 'une page';
    const list = h('div');
    const count = h('span', { class: 'result-count' });
    const search = h('input', { type: 'search', placeholder: `Rechercher dans les ${plural}…`, 'aria-label': `Rechercher dans les ${plural}`, onInput: renderList });
    function renderList() {
      const query = search.value.trim().toLocaleLowerCase('fr');
      const filtered = contents.filter(item => `${item.title} ${item.slug}`.toLocaleLowerCase('fr').includes(query));
      count.textContent = `${filtered.length} ${kind === 'article' ? 'article' : 'page'}${filtered.length > 1 ? 's' : ''}`;
      list.replaceChildren(filtered.length ? contentTable(filtered) : query ? empty('Aucun résultat', 'Essayez un autre titre ou une autre adresse.', null, 'search') : empty(`Créez votre ${kind === 'article' ? 'premier article' : 'première page'}`, 'Vos contenus sont enregistrés en brouillon. Vous choisissez quand les rendre visibles sur votre vitrine.', canEditContent() ? link(`Créer ${singular}`, newContentLink(kind), 'button primary', 'plus') : null, kind));
    }
    renderList();
    state.main.replaceChildren(heading(kind === 'article' ? 'Articles' : 'Pages', kind === 'article' ? 'Les actualités et les idées qui font vivre votre site.' : 'Organisez et publiez le contenu de votre vitrine.', [canEditContent() ? link(`Créer ${singular}`, newContentLink(kind), 'button primary', 'plus') : null]), h('section', { class: 'panel' }, h('div', { class: 'list-toolbar' }, h('div', { class: 'search-wrap' }, icon('search'), search), count), list));
  }

  function structureStatus(page) {
    const changed = page.status === 'published' && page.has_unpublished_changes;
    const labels = { automatic: 'Page du modèle', published: 'Publié', draft: 'Brouillon', disabled: 'Blog désactivé', private: 'Espace privé' };
    return h('span', { class: `status${changed ? ' changed' : page.status === 'published' ? ' published' : page.status === 'private' ? ' private' : ''}` }, changed ? 'Modifications en brouillon' : labels[page.status] || 'Page du modèle');
  }

  function pageActions(page) {
    const actions = h('div', { class: 'page-row-actions' });
    if (page.content_id) {
      actions.append(link(canEditContent() ? 'Concevoir' : 'Aperçu visuel', builderLink(page.content_id), 'button secondary small', 'sites'), link('Détails', contentEditorLink({ id: page.content_id, kind: 'page' }), 'button ghost small', 'edit'));
    } else if (page.id === 'home') {
      actions.append(link(canEditContent() ? 'Concevoir' : 'Aperçu visuel', builderLink('home'), 'button secondary small', 'sites'));
    } else if (page.id === 'about' && canEditContent()) {
      const createAbout = button('Concevoir', 'secondary small', 'sites', async () => {
        createAbout.disabled = true;
        try { const content = await api(endpoint('/contents'), { method: 'POST', body: { kind: 'page', title: 'À propos', slug: 'a-propos', excerpt: '', body: '' } }); window.location.assign(builderLink(content.id)); }
        catch (error) { toast(error.message, true); createAbout.disabled = false; }
      });
      actions.append(createAbout);
    } else if (page.id === 'blog') {
      actions.append(link(state.settings.blog_enabled ? 'Articles' : canManageTeam() ? 'Activer le blog' : 'Module Blog', adminLink(state.settings.blog_enabled ? 'articles' : 'modules'), 'button secondary small', state.settings.blog_enabled ? 'article' : 'modules'));
    }
    const path = page.kind === 'system' ? page.path : page.published_path;
    if (path) actions.append(h('a', { class: 'button ghost small', href: publicPageLink(path), target: '_blank', rel: 'noopener', 'aria-label': `${page.id === 'account' ? 'Accéder à' : 'Voir'} ${page.title}` }, icon('external'), page.id === 'account' ? 'Accéder' : 'Voir'));
    return actions;
  }

  async function sitePages() {
    const structure = await api(endpoint('/structure'));
    const pages = structure.pages;
    const list = h('div');
    const count = h('span', { class: 'result-count pages-count', role: 'status', 'aria-live': 'polite' });
    const search = h('input', { type: 'search', placeholder: 'Rechercher une page ou une adresse…', 'aria-label': 'Rechercher dans les pages', onInput: renderPages });
    function renderPages() {
      const query = search.value.trim().toLocaleLowerCase('fr');
      const filtered = pages.filter(page => `${page.title} ${page.path} ${page.description}`.toLocaleLowerCase('fr').includes(query));
      count.textContent = `${filtered.length} page${filtered.length > 1 ? 's' : ''}${query ? ` sur ${pages.length}` : ''}`;
      if (!filtered.length) { list.replaceChildren(empty('Aucune page trouvée', 'Essayez un autre titre ou une autre adresse.', null, 'search')); return; }
      const rows = h('tbody');
      for (const page of filtered) {
        const symbol = page.id === 'home' ? 'home' : page.id === 'blog' ? 'article' : page.id === 'account' ? 'lock' : 'page';
        const title = h('div', { class: 'content-title' }, h('span', { class: 'document-icon' }, icon(symbol)), h('div', {}, h('strong', {}, page.title), h('span', { class: 'content-slug' }, page.path)));
        rows.append(h('tr', {}, h('td', {}, title, h('p', { class: 'page-row-description' }, page.description)), h('td', { class: 'hide-mobile' }, h('span', { class: 'page-kind' }, page.kind === 'system' ? 'Page du modèle' : 'Page personnalisée')), h('td', {}, structureStatus(page)), h('td', {}, pageActions(page))));
      }
      list.replaceChildren(h('div', { class: 'table-scroll' }, h('table', { class: 'content-table site-pages-table' }, h('thead', {}, h('tr', {}, h('th', { scope: 'col' }, 'Page'), h('th', { scope: 'col', class: 'hide-mobile' }, 'Type'), h('th', { scope: 'col' }, 'Statut'), h('th', { scope: 'col' }, 'Actions'))), rows)));
    }
    renderPages();
    state.main.replaceChildren(
      heading('Pages', 'Toutes les pages de votre site, des pages du modèle à vos contenus personnalisés.', [link('Modèle du site', adminLink('template'), 'button secondary', 'sites'), canEditContent() ? link('Créer une page', newContentLink('page'), 'button primary', 'plus') : null]),
      h('section', { class: 'panel' }, h('div', { class: 'list-toolbar' }, h('div', { class: 'search-wrap' }, icon('search'), search), count), list),
      h('p', { class: 'pages-help' }, 'L’en-tête et le pied de page sont communs aux pages de la vitrine. ', link('Voir le modèle et ses menus', adminLink('template'), 'text-link', 'arrow'))
    );
  }

  async function templateView() {
    const structure = await api(endpoint('/structure'));
    const template = structure.template;
    let selectedPage = 'home';
    let selectedArea = 'full';
    let selectedDevice = 'desktop';
    let loadingTimeout;
    const pageNames = { home: 'Accueil', about: 'À propos', blog: 'Blog' };
    const areaNames = { full: 'Page entière', header: 'En-tête', footer: 'Pied de page' };
    const loading = h('div', { class: 'template-loading', role: 'status', 'aria-live': 'polite' }, h('span', { class: 'domain-spinner', 'aria-hidden': 'true' }), 'Chargement de l’aperçu…');
    const previewError = h('div', { class: 'template-error', role: 'alert', hidden: true });
    const frame = h('iframe', { class: 'template-frame', title: 'Aperçu du modèle du site', sandbox: 'allow-same-origin', referrerpolicy: 'same-origin', tabindex: '-1' });
    const viewport = h('div', { class: 'template-viewport' }, frame);
    const stage = h('div', { class: 'template-stage', 'aria-busy': 'true' }, loading, previewError, viewport);
    const caption = h('p', { class: 'template-caption' });
    function failPreview() {
      window.clearTimeout(loadingTimeout);
      stage.setAttribute('aria-busy', 'false'); loading.hidden = true; viewport.hidden = true; previewError.hidden = false;
      previewError.replaceChildren(h('strong', {}, 'L’aperçu n’a pas pu être chargé.'), h('p', {}, 'Réessayez ou ouvrez la vitrine dans un nouvel onglet.'), button('Réessayer', 'secondary small', null, loadPreview));
    }
    frame.addEventListener('load', () => {
      if (!frame.getAttribute('src')) return;
      try {
        if (!frame.contentDocument?.body?.hasAttribute('data-template-preview')) { failPreview(); return; }
        window.clearTimeout(loadingTimeout);
        stage.setAttribute('aria-busy', 'false'); loading.hidden = true; previewError.hidden = true; viewport.hidden = false;
      } catch { failPreview(); }
    });
    frame.addEventListener('error', failPreview);
    function loadPreview() {
      window.clearTimeout(loadingTimeout);
      loading.hidden = false; previewError.hidden = true; viewport.hidden = true; stage.setAttribute('aria-busy', 'true');
      stage.className = `template-stage template-area-${selectedArea}`;
      viewport.className = `template-viewport${selectedDevice === 'mobile' ? ' is-mobile' : ''}`;
      frame.title = `Aperçu ${pageNames[selectedPage]} — ${areaNames[selectedArea]}`;
      caption.textContent = `${pageNames[selectedPage]} · ${areaNames[selectedArea]} · ${selectedDevice === 'mobile' ? 'Format mobile' : 'Largeur disponible'}. Aperçu de la version visible par vos visiteurs.`;
      const previewUrl = endpoint(`/template-preview?${new URLSearchParams({ page: selectedPage, area: selectedArea })}`);
      frame.src = isExtranet ? '/extranet' + previewUrl : previewUrl;
      loadingTimeout = window.setTimeout(failPreview, 20000);
    }
    const pageSelect = h('select', { id: 'template-page', onChange: e => { selectedPage = e.target.value; loadPreview(); } }, Object.entries(pageNames).map(([value, text]) => h('option', { value }, text)));
    const areaButtons = h('div', { class: 'template-segments', role: 'group', 'aria-label': 'Zone du modèle à afficher' });
    const areaControls = [];
    for (const [area, label] of Object.entries(areaNames)) {
      const control = button(label, 'ghost small', null, () => { selectedArea = area; for (const [key, item] of areaControls) item.setAttribute('aria-pressed', String(key === area)); loadPreview(); });
      control.setAttribute('aria-pressed', String(area === selectedArea)); areaControls.push([area, control]); areaButtons.append(control);
    }
    const deviceButtons = h('div', { class: 'template-segments template-devices', role: 'group', 'aria-label': 'Format de l’aperçu' });
    const deviceControls = [];
    for (const [device, label] of [['desktop', 'Ordinateur'], ['mobile', 'Mobile']]) {
      const control = button(label, 'ghost small', null, () => {
        selectedDevice = device;
        for (const [key, item] of deviceControls) item.setAttribute('aria-pressed', String(key === device));
        viewport.className = `template-viewport${device === 'mobile' ? ' is-mobile' : ''}`;
        caption.textContent = `${pageNames[selectedPage]} · ${areaNames[selectedArea]} · ${device === 'mobile' ? 'Format mobile' : 'Largeur disponible'}. Aperçu de la version visible par vos visiteurs.`;
      });
      control.setAttribute('aria-pressed', String(device === selectedDevice)); deviceControls.push([device, control]); deviceButtons.append(control);
    }
    function menuPanel(title, subtitle, section, area) {
      const entries = h('ul', { class: 'template-menu-list' });
      for (const item of section.links) entries.append(h('li', {}, h('a', { class: 'template-menu-label', href: publicPageLink(item.path), target: '_blank', rel: 'noopener' }, item.icon === 'user-round' ? h('img', { src: '/assets/icons/user-round.svg', alt: '', width: '15', height: '15' }) : icon('page'), item.label), h('code', {}, item.path)));
      const inspect = button(`Voir ${area === 'header' ? 'l’en-tête' : 'le pied de page'}`, 'secondary small', null, () => { selectedArea = area; for (const [key, item] of areaControls) item.setAttribute('aria-pressed', String(key === area)); loadPreview(); previewPanel.scrollIntoView({ behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth', block: 'start' }); });
      return h('section', { class: 'panel template-region' }, h('div', { class: 'panel-header' }, h('div', {}, h('h2', {}, title), h('p', {}, subtitle)), inspect), entries);
    }
    const previewPanel = h('section', { class: 'panel template-preview-panel', 'aria-label': 'Aperçu du modèle' }, h('div', { class: 'template-toolbar' }, h('div', { class: 'template-page-select' }, h('label', { for: 'template-page' }, 'Page affichée'), pageSelect), areaButtons, deviceButtons), stage, caption);
    state.main.replaceChildren(
      heading('Modèle du site', 'Retrouvez la structure de votre vitrine, son en-tête et son pied de page.', [link(canManageTeam() ? 'Personnaliser le modèle' : 'Consulter le modèle', builderLink('template'), 'button primary', 'sites'), h('a', { class: 'button secondary', href: publicBase() || '/', target: '_blank', rel: 'noopener' }, icon('external'), 'Ouvrir la vitrine')]),
      h('section', { class: 'panel template-summary' }, h('div', {}, h('p', { class: 'eyebrow' }, 'MODÈLE ACTUEL'), h('h2', {}, template.name), h('p', {}, 'Un accueil clair, une navigation commune et les contenus de votre site.')), h('span', { class: 'status published' }, 'Utilisé sur ce site')),
      previewPanel,
      h('div', { class: 'template-regions' }, menuPanel('En-tête · Header', 'Identité du site et menu principal', template.header, 'header'), menuPanel('Pied de page · Footer', 'Liens de navigation en bas de chaque page', template.footer, 'footer')),
      h('p', { class: 'pages-help' }, 'Ces deux parties sont communes aux pages de la vitrine. Personnalisez la disposition du logo, les liens du menu et le pied de page dans l’éditeur visuel. ', link('Gérer les pages du site', adminLink('pages'), 'text-link', 'arrow'))
    );
    loadPreview();
  }

  async function visualBuilder() {
    const target = url.searchParams.get('target') || 'home';
    if (!['home', 'template'].includes(target) && !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(target)) throw new Error('Cette page est introuvable. Revenez à la liste des pages.');
    let title = target === 'home' ? 'Page d’accueil' : 'Modèle du site';
    if (target !== 'home' && target !== 'template') {
      const content = await api(endpoint(`/contents/${encodeURIComponent(target)}`));
      if (content.kind !== 'page') throw new Error('L’éditeur visuel est disponible pour les pages du site.');
      title = content.title;
    }
    if (!window.CMSBuilder) throw new Error('L’éditeur visuel n’a pas pu être chargé. Actualisez la page.');
    await window.CMSBuilder.mount({
      root: state.main, target, title, site: state.site, api: (path, options = {}) => api(path, { ...options, keepDraft: true }), endpoint, authHref: authLink('/admin/login'), canEdit: canEditContent(), canTemplateEdit: canManageTeam(),
      backHref: adminLink(target === 'template' ? 'template' : 'pages'), templateHref: builderLink('template'),
      onDirty: value => { state.dirty = value; }, toast,
      preview: async (path, body) => {
        let response;
        try { response = await fetch((isExtranet ? '/extranet' : '') + path, { method: 'POST', credentials: 'same-origin', cache: 'no-store', headers: { 'Content-Type': 'application/json', Accept: 'text/html' }, body: JSON.stringify(body) }); }
        catch { throw new Error('Connexion impossible. Réessayez pour actualiser l’aperçu.'); }
        if (!response.ok) {
          let message = 'L’aperçu n’a pas pu être actualisé.';
          try { message = (await response.json()).error || message; } catch { /* Preserve the readable error. */ }
          if (response.status === 401) message = 'Votre session a expiré. Enregistrez pour afficher les options de reconnexion sans perdre votre brouillon.';
          throw new ApiError(message, response.status);
        }
        return response.text();
      },
    });
  }

  async function editor() {
    const fresh = paths[2] === 'new';
    if (fresh && !canEditContent()) { state.main.replaceChildren(heading('Consultation des contenus', 'Votre rôle Lecture seule ne permet pas de créer un contenu.'), readOnlyNotice(), link('Retour aux pages', adminLink('pages'), 'button secondary', 'back')); return; }
    let content = fresh ? { kind: url.searchParams.get('kind') === 'article' ? 'article' : 'page', title: '', slug: '', excerpt: '', body: '', published: false } : await api(endpoint(`/contents/${encodeURIComponent(paths[2])}`));
    if (content.kind === 'article' && !state.settings.blog_enabled) return listContents('article');
    if (fresh && content.kind === 'page' && url.searchParams.get('preset') === 'about') { content.title = 'À propos'; content.slug = 'a-propos'; }
    const isArticle = content.kind === 'article';
    const editable = canEditContent();
    const form = h('form', { class: 'editor-form' });
    const title = field('Titre', 'title', { value: content.title, required: true, maxlength: 200, placeholder: isArticle ? 'Le titre de votre article' : 'Le titre de votre page', class: 'title-input' });
    const slug = field('Adresse de la page', 'slug', { value: content.slug, required: true, maxlength: 100, pattern: '[a-z0-9]+(-[a-z0-9]+)*', placeholder: 'votre-adresse', hint: 'Lettres minuscules, chiffres et tirets. Exemple : a-propos.' });
    const excerpt = field('Résumé', 'excerpt', { value: content.excerpt, maxlength: 500, textarea: true, rows: 3, placeholder: 'Présentez ce contenu en quelques mots.', hint: 'Ce texte apparaît dans la liste des contenus de la vitrine.' });
    const body = field('Contenu', 'body', { value: content.body, maxlength: 100000, textarea: true, rows: 18, class: 'body-input', placeholder: 'Rédigez votre contenu ici…', hint: 'Texte simple. Séparez vos paragraphes avec une ligne vide.' });
    let manualSlug = Boolean(content.slug);
    title.input.addEventListener('input', () => { if (!manualSlug) slug.input.value = slugify(title.input.value); });
    slug.input.addEventListener('input', () => { manualSlug = true; });
    const serialize = () => ({ title: title.input.value.trim(), slug: slug.input.value.trim(), excerpt: excerpt.input.value.trim(), body: body.input.value });
    let baseline = JSON.stringify(serialize());
    const dirtyLabel = h('span', { class: 'hint' });
    const errorBox = formError();
    const publication = h('div');
    const details = h('dl', { class: 'side-details' });
    const saveButton = h('button', { type: 'submit', class: 'button secondary' }, 'Enregistrer le brouillon');
    const publishButton = button(content.published ? 'Publier les modifications' : 'Publier', 'primary full', 'check', () => runAction('publish'));
    const unpublishButton = button('Retirer de la vitrine', 'secondary full', null, () => runAction('unpublish'));
    const deleteButton = button('Supprimer ce contenu', 'danger full', null, confirmDeletion);
    const preview = h('a', { class: 'button ghost full', target: '_blank', rel: 'noopener' }, icon('external'), 'Voir la version publiée');
    const wordCount = h('p', { class: 'hint' });
    const visualEntry = h('a', { class: 'button secondary', hidden: isArticle || !content.id }, icon('sites'), editable ? 'Éditeur visuel' : 'Aperçu visuel');
    const visualNote = h('div', { class: 'notice builder-editor-note', hidden: !content.layout }, h('p', {}, 'Cette page est composée de blocs. Modifiez son contenu dans l’éditeur visuel. Le titre, l’adresse et le résumé ci-dessous restent les informations de la page.'));
    const updateDirty = () => { state.dirty = editable && JSON.stringify(serialize()) !== baseline; dirtyLabel.textContent = state.dirty ? 'Modifications non enregistrées' : content.id ? 'Toutes les modifications sont enregistrées' : 'Nouveau brouillon'; wordCount.textContent = `${body.input.value.trim() ? body.input.value.trim().split(/\s+/u).length : 0} mots`; };
    form.addEventListener('input', updateDirty);
    function updatePublication() {
      visualEntry.hidden = isArticle || !content.id; if (content.id) visualEntry.href = builderLink(content.id);
      visualNote.hidden = !content.layout; body.node.hidden = Boolean(content.layout); wordCount.hidden = Boolean(content.layout); body.input.readOnly = !editable || Boolean(content.layout);
      publication.replaceChildren(status(content), h('p', {}, !editable ? content.published ? 'Une version de ce contenu est visible sur la vitrine.' : 'Ce brouillon n’est pas visible sur la vitrine.' : content.published ? content.has_unpublished_changes ? 'La version publiée reste visible. Publiez à nouveau pour mettre vos modifications en ligne.' : 'Ce contenu est visible sur la vitrine. Vous pouvez le modifier sans changer la version publiée.' : 'Ce contenu reste privé jusqu’à sa publication.'));
      publishButton.replaceChildren(icon('check'), content.published ? 'Publier les modifications' : 'Publier');
      unpublishButton.hidden = !content.published;
      deleteButton.hidden = !content.id;
      preview.hidden = !content.published;
      if (content.published) preview.href = publicLink(content);
      details.replaceChildren(h('dt', {}, 'Type'), h('dd', {}, isArticle ? 'Article' : 'Page'), h('dt', {}, 'Dernière modification'), h('dd', {}, date(content.updated_at, true)), h('dt', {}, 'Dernière publication'), h('dd', {}, date(content.published_at, true)));
      updateDirty();
    }
    async function saveDraft() {
      if (!form.reportValidity()) return false;
      const values = serialize();
      if (content.id && JSON.stringify(values) === baseline) return true;
      const saved = await api(endpoint(content.id ? `/contents/${encodeURIComponent(content.id)}` : '/contents'), { method: content.id ? 'PUT' : 'POST', body: content.id ? { ...values, expected_revision: content.revision } : { ...values, kind: content.kind } });
      content = saved;
      baseline = JSON.stringify(serialize());
      state.dirty = false;
      window.history.replaceState(null, '', contentEditorLink(content));
      updatePublication(); return true;
    }
    let busy = false;
    async function runAction(action) {
      if (busy || !editable) return;
      if (action !== 'unpublish' && !form.reportValidity()) return;
      busy = true; errorBox.hidden = true; setBusy(form, true);
      try {
        if (action === 'save' || action === 'publish') {
          // Validation happened before disabling the form, so saveDraft can use its values.
          if (!(await saveDraft())) return;
        }
        if (action === 'publish' || action === 'unpublish') content = await api(endpoint(`/contents/${encodeURIComponent(content.id)}/${action}`), { method: 'POST', body: { expected_revision: content.revision } });
        updatePublication();
        toast(action === 'publish' ? 'Le contenu est publié sur la vitrine.' : action === 'unpublish' ? 'Le contenu a été retiré de la vitrine.' : 'Le brouillon est enregistré.');
      } catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; }
      finally { busy = false; setBusy(form, false); }
    }
    function confirmDeletion() {
      const dialog = h('dialog', { class: 'dialog', 'aria-labelledby': 'delete-title' });
      const close = () => { dialog.close(); dialog.remove(); };
      const confirm = button('Supprimer définitivement', 'danger', null, async () => {
        confirm.disabled = true;
        try { await api(endpoint(`/contents/${encodeURIComponent(content.id)}`), { method: 'DELETE' }); state.dirty = false; window.location.assign(adminLink(isArticle ? 'articles' : 'pages')); }
        catch (error) { toast(error.message, true); confirm.disabled = false; }
      });
      dialog.append(h('h2', { id: 'delete-title' }, 'Supprimer ce contenu ?'), h('p', {}, `« ${content.title} » sera définitivement supprimé, y compris sa version publiée. Cette action ne peut pas être annulée.`), h('div', { class: 'dialog-actions' }, button('Annuler', 'secondary', null, close), confirm));
      dialog.addEventListener('cancel', close); document.body.append(dialog); dialog.showModal();
    }
    form.addEventListener('submit', event => { event.preventDefault(); runAction('save'); });
    form.append(editable ? null : readOnlyNotice(), visualNote, errorBox, h('div', { class: 'editor-grid' }, h('section', { class: 'panel editor-panel' }, h('span', { class: 'editor-kind' }, icon(isArticle ? 'article' : 'page'), isArticle ? 'ARTICLE' : 'PAGE'), title.node, excerpt.node, body.node, wordCount), h('aside', { class: 'editor-side' }, h('section', { class: 'panel side-panel' }, h('h2', {}, 'Publication'), h('div', { class: 'publication-state' }, publication), editable ? publishButton : null, editable ? unpublishButton : null, preview), h('section', { class: 'panel side-panel' }, h('h2', {}, 'Adresse & détails'), slug.node, details), editable ? h('section', { class: 'panel side-panel' }, h('h2', {}, 'Gestion du contenu'), deleteButton, h('p', { class: 'hint' }, 'Enregistrez un brouillon pour conserver vos modifications sans les publier.')) : null)));
    updatePublication();
    state.main.replaceChildren(heading(fresh ? isArticle ? 'Nouvel article' : 'Nouvelle page' : editable ? isArticle ? 'Modifier l’article' : 'Modifier la page' : isArticle ? 'Consulter l’article' : 'Consulter la page', isArticle ? 'Partagez une actualité avec vos visiteurs.' : 'Donnez vie à votre vitrine, une page à la fois.', [link('Retour', adminLink(isArticle ? 'articles' : 'pages'), 'button ghost', 'back'), visualEntry, editable ? saveButton : null]), editable ? h('div', { class: 'save-state' }, dirtyLabel) : null, form);
    // The persistent action sits in the heading but submits this editor form.
    if (!editable) for (const item of [title.input, slug.input, excerpt.input, body.input]) { item.readOnly = true; item.setAttribute('aria-readonly', 'true'); }
    form.id = 'content-editor'; saveButton.setAttribute('form', form.id);
  }

  async function settings() {
    const data = state.settings;
    const form = h('form', { class: 'panel settings-panel' });
    const name = field('Nom de la vitrine', 'site_name', { value: data.site_name, required: true, maxlength: 200, hint: 'Le nom affiché à vos visiteurs.' });
    const description = field('Présentation', 'description', { value: data.description, maxlength: 500, textarea: true, rows: 4, placeholder: 'Présentez votre activité en quelques phrases.', hint: 'Ce texte est affiché sur la page d’accueil de la vitrine.' });
    const errorBox = formError();
    const save = h('button', { type: 'submit', class: 'button primary' }, 'Enregistrer les réglages');
    const initial = () => JSON.stringify([name.input.value, description.input.value]); let baseline = initial();
    form.addEventListener('input', () => { state.dirty = initial() !== baseline; });
    form.addEventListener('submit', async event => {
      event.preventDefault(); if (!canManageTeam()) return; setBusy(form, true); errorBox.hidden = true;
      try { const latest = await api(endpoint('/settings')); state.settings = await api(endpoint('/settings'), { method: 'PUT', body: { site_name: name.input.value.trim(), description: description.input.value.trim(), blog_enabled: latest.blog_enabled } }); baseline = initial(); state.dirty = false; toast('Les réglages de la vitrine sont enregistrés.'); }
      catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; }
      finally { setBusy(form, false); }
    });
    form.append(h('h2', {}, 'Identité de la vitrine'), h('p', { class: 'page-subtitle' }, 'Les informations essentielles pour accueillir vos visiteurs.'), canManageTeam() ? null : readOnlyNotice(true), errorBox, name.node, description.node, canManageTeam() ? h('div', { class: 'settings-footer' }, save) : null);
    if (!canManageTeam()) { name.input.readOnly = true; description.input.readOnly = true; }
    state.main.replaceChildren(heading('Réglages', `Personnalisez la présentation de ${state.site.name}.`), form);
  }

  async function modules() {
    const enabled = state.settings.blog_enabled;
    const errorBox = formError();
    const action = button(enabled ? 'Désactiver le Blog' : 'Activer le Blog', enabled ? 'secondary' : 'primary', enabled ? null : 'plus', async () => {
      if (enabled && !window.confirm('Désactiver le Blog ? Les articles seront conservés, mais ne seront plus accessibles sur la vitrine tant que le module est désactivé.')) return;
      action.disabled = true; errorBox.hidden = true;
      try { const latest = await api(endpoint('/settings')); state.settings = await api(endpoint('/settings'), { method: 'PUT', body: { site_name: latest.site_name, description: latest.description, blog_enabled: !enabled } }); window.location.assign(adminLink('modules')); }
      catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; action.disabled = false; }
    });
    state.main.replaceChildren(heading('Modules', 'Ajoutez les fonctions utiles à chaque site, au moment où vous en avez besoin.'), h('section', { class: 'panel module-card' }, h('div', { class: 'module-icon' }, icon('article')), h('div', { class: 'module-description' }, h('div', { class: 'module-heading' }, h('h2', {}, 'Blog'), h('span', { class: `status${enabled ? ' published' : ''}` }, enabled ? 'Activé' : 'Désactivé')), h('p', {}, 'Publiez des actualités, des conseils et des articles pour faire vivre votre vitrine. Préparez vos brouillons avant de les mettre en ligne.'), h('p', { class: 'hint' }, 'La désactivation conserve les articles et masque le blog sur la vitrine.'), errorBox, h('div', { class: 'module-actions' }, canManageTeam() ? action : h('p', { class: 'hint' }, 'Seul un administrateur peut activer ou désactiver ce module.'), enabled ? link(canEditContent() ? 'Gérer les articles' : 'Consulter les articles', adminLink('articles'), 'button ghost', 'arrow') : null))), h('p', { class: 'modules-note' }, 'Les pages de votre vitrine sont toujours disponibles. D’autres modules pourront être ajoutés au CMS par la suite.'));
  }

  async function teamUsers() {
    if (!canManageTeam()) {
      state.main.replaceChildren(heading('Utilisateurs', 'La gestion de l’équipe est réservée aux administrateurs de ce site.'), link('Retour à la vue d’ensemble', adminLink(), 'button secondary', 'back'));
      return;
    }
    const members = h('div', { class: 'team-members' });
    const pending = h('div', { class: 'team-invitations' });
    const pageError = formError();
    const result = h('div', { class: 'team-invite-result', 'aria-live': 'polite' });
    const count = h('span', { class: 'hint', role: 'status' });
    const form = h('form', { class: 'panel team-invite-form' });
    const name = field('Nom du collaborateur', 'invite-name', { required: true, maxlength: 100, autocomplete: 'off', placeholder: 'Prénom et nom' });
    const email = field('Adresse e-mail', 'invite-email', { required: true, type: 'email', maxlength: 254, autocomplete: 'off', placeholder: 'prenom@entreprise.fr' });
    const role = roleSelect('invite-role', 'editor');
    const description = h('p', { class: 'hint', id: 'invite-role-description' }, roleDescriptions.editor);
    role.setAttribute('aria-describedby', 'invite-role-description');
    role.addEventListener('change', () => { description.textContent = roleDescriptions[role.value]; });
    const formErrors = formError();
    const submit = h('button', { type: 'submit', class: 'button primary' }, icon('plus'), 'Créer l’invitation');
    const refresh = button('Actualiser', 'secondary small', null, () => load().catch(error => { pageError.textContent = error.message; pageError.hidden = false; }));
    let currentUserId = null;
    function roleSelect(id, value) {
      const select = h('select', { id, name: 'role', required: true, class: 'team-role-select' }, ['admin', 'editor', 'viewer'].map(key => h('option', { value: key }, roleLabels[key])));
      select.value = value;
      return select;
    }
    function openMemberDialog(member) {
      const dialog = h('dialog', { class: 'dialog team-dialog', 'aria-labelledby': 'member-dialog-title' });
      const editing = h('form');
      const selectedRole = roleSelect('member-role', member.role);
      const active = h('input', { type: 'checkbox', id: 'member-active', checked: member.active });
      const error = formError();
      const explanation = h('p', { class: 'hint' }, roleDescriptions[member.role]);
      selectedRole.addEventListener('change', () => { explanation.textContent = roleDescriptions[selectedRole.value]; });
      const save = h('button', { type: 'submit', class: 'button primary' }, 'Enregistrer');
      const close = () => { dialog.close(); dialog.remove(); };
      editing.append(h('h2', { id: 'member-dialog-title' }, 'Modifier l’accès'), h('p', { class: 'team-member-identity' }, member.name, h('br'), member.email), error,
        h('div', { class: 'field' }, h('label', { for: 'member-role' }, 'Rôle dans l’équipe'), selectedRole, explanation),
        h('label', { class: 'inline-checkbox', for: 'member-active' }, active, 'Autoriser la connexion à l’extranet'),
        h('p', { class: 'hint' }, 'La désactivation déconnecte ce membre. Ses contenus sont conservés. Au moins un administrateur doit rester actif.'),
        h('div', { class: 'dialog-actions' }, button('Annuler', 'secondary', null, close), save));
      editing.addEventListener('submit', async event => {
        event.preventDefault(); if (!editing.reportValidity()) return;
        error.hidden = true; setBusy(editing, true);
        try {
          await api(endpoint(`/users/${encodeURIComponent(member.id)}`), { method: 'PUT', body: { role: selectedRole.value, active: active.checked } });
          close(); toast('L’accès du collaborateur est mis à jour.');
          if (isExtranet && member.id === currentUserId) window.location.reload();
          else await load();
        } catch (failure) { error.textContent = failure.message; error.hidden = false; }
        finally { setBusy(editing, false); }
      });
      dialog.addEventListener('cancel', close); dialog.append(editing); document.body.append(dialog); dialog.showModal();
    }
    function openPasswordDialog(member) {
      const dialog = h('dialog', { class: 'dialog team-dialog', 'aria-labelledby': 'password-dialog-title' });
      const editing = h('form');
      const password = field('Nouveau mot de passe', 'member-password', { type: 'password', minlength: 12, maxlength: 256, required: true, autocomplete: 'new-password', hint: 'Au moins 12 caractères. Le mot de passe actuel n’est jamais affiché.' });
      const reveal = h('input', { type: 'checkbox', id: 'reveal-member-password', onChange: () => { password.input.type = reveal.checked ? 'text' : 'password'; } });
      const error = formError();
      const close = () => { password.input.value = ''; dialog.close(); dialog.remove(); };
      const submit = h('button', { type: 'submit', class: 'button primary' }, 'Réinitialiser le mot de passe');
      editing.append(h('h2', { id: 'password-dialog-title' }, 'Nouveau mot de passe'), h('p', { class: 'team-member-identity' }, member.name, h('br'), member.email), error, password.node,
        h('label', { class: 'inline-checkbox', for: 'reveal-member-password' }, reveal, 'Afficher le nouveau mot de passe'),
        h('p', { class: 'hint' }, 'Les sessions de ce membre seront déconnectées. Transmettez-lui son nouveau mot de passe par un canal privé.'),
        h('div', { class: 'dialog-actions' }, button('Annuler', 'secondary', null, close), submit));
      editing.addEventListener('submit', async event => {
        event.preventDefault(); if (!editing.reportValidity()) return;
        error.hidden = true; setBusy(editing, true);
        try { await api(endpoint(`/users/${encodeURIComponent(member.id)}/password`), { method: 'POST', body: { password: password.input.value } }); close(); toast('Le mot de passe de ce membre est modifié.'); }
        catch (failure) { error.textContent = failure.message; error.hidden = false; }
        finally { setBusy(editing, false); }
      });
      dialog.addEventListener('cancel', close); dialog.append(editing); document.body.append(dialog); dialog.showModal();
    }
    async function load() {
      pageError.hidden = true; refresh.disabled = true;
      try {
        const data = await api(endpoint('/users'));
        currentUserId = data.current_user_id;
        count.textContent = `${data.users.length} membre${data.users.length > 1 ? 's' : ''} · ${data.users.filter(user => user.active).length} actif${data.users.filter(user => user.active).length > 1 ? 's' : ''}`;
        members.replaceChildren();
        if (!data.users.length) members.append(empty('L’équipe n’est pas encore activée', 'Activez d’abord le compte principal du client depuis les informations de son site. Vous pourrez ensuite inviter ses collaborateurs.', !isExtranet ? link('Informations du client', `/admin/sites/${encodeURIComponent(state.site.id)}`, 'button secondary', 'arrow') : null, 'lock'));
        for (const member of data.users) {
          const controls = h('div', { class: 'team-member-actions' }, button('Modifier l’accès', 'secondary small', 'edit', () => openMemberDialog(member)));
          if (!isExtranet) controls.append(button('Mot de passe', 'ghost small', 'lock', () => openPasswordDialog(member)));
          members.append(h('article', { class: 'team-member' }, h('span', { class: 'team-avatar', 'aria-hidden': 'true' }, (member.name || member.email).slice(0, 1).toUpperCase()),
            h('div', { class: 'team-member-info' }, h('h3', {}, member.name, member.id === currentUserId && isExtranet ? h('span', { class: 'team-you' }, 'Vous') : null), h('p', {}, member.email), h('div', { class: 'team-member-badges' }, h('span', { class: 'team-role' }, roleLabels[member.role] || member.role), h('span', { class: `status${member.active ? ' published' : ''}` }, member.active ? 'Actif' : 'Désactivé'), member.is_primary ? h('span', { class: 'team-primary' }, 'Compte principal') : null)), controls));
        }
        submit.disabled = !data.users.length;
        pending.replaceChildren(h('div', { class: 'panel-header' }, h('div', {}, h('h2', {}, 'Invitations en attente'), h('p', {}, 'Les liens expirent automatiquement et ne fonctionnent qu’une fois.'))));
        if (!data.invitations.length) pending.append(h('p', { class: 'team-pending-empty' }, 'Aucune invitation en attente.'));
        for (const invitation of data.invitations) {
          const revoke = button('Annuler l’invitation', 'ghost small', null, async () => {
            revoke.disabled = true;
            try { await api(endpoint(`/users/invitations/${encodeURIComponent(invitation.id)}`), { method: 'DELETE' }); toast('L’invitation est annulée.'); await load(); }
            catch (error) { pageError.textContent = error.message; pageError.hidden = false; revoke.disabled = false; }
          });
          pending.append(h('article', { class: 'team-pending-row' }, h('div', {}, h('strong', {}, invitation.name || invitation.email), h('p', {}, invitation.email), h('p', { class: 'hint' }, `${roleLabels[invitation.role]} · Expire le ${date(invitation.expires_at, true)}`)), revoke));
        }
      } finally { refresh.disabled = false; }
    }
    form.append(h('h2', {}, 'Inviter un collaborateur'), h('p', { class: 'page-subtitle' }, 'Choisissez son rôle. Il définira lui-même son mot de passe grâce au lien privé.'), formErrors, name.node, email.node, h('div', { class: 'field' }, h('label', { for: 'invite-role' }, 'Rôle'), role, description), submit, h('p', { class: 'hint' }, 'Aucun e-mail n’est envoyé automatiquement. Copiez le lien et transmettez-le au collaborateur.'), result);
    form.addEventListener('submit', async event => {
      event.preventDefault(); if (!form.reportValidity()) return;
      formErrors.hidden = true; setBusy(form, true);
      try {
        const invitation = await api(endpoint('/users/invitations'), { method: 'POST', body: { email: email.input.value.trim(), name: name.input.value.trim(), role: role.value } });
        const value = h('input', { type: 'text', value: invitation.activation_url, readonly: true, class: 'activation-link', 'aria-label': 'Lien privé d’invitation' });
        const copy = button('Copier le lien', 'secondary small', null, async () => {
          try { await navigator.clipboard.writeText(invitation.activation_url); copy.textContent = 'Lien copié'; }
          catch { value.focus(); value.select(); toast('Sélectionnez et copiez le lien affiché.'); }
        });
        result.replaceChildren(h('div', { class: 'domain-ai-result' }, h('strong', {}, 'Invitation prête à transmettre'), h('p', {}, `Pour ${email.input.value.trim()}. Conservez ce lien avant de quitter la page.`), value, h('p', { class: 'hint' }, `Valable jusqu’au ${date(invitation.expires_at, true)}.`), copy));
        name.input.value = ''; email.input.value = ''; toast('Le lien privé d’invitation est créé.'); await load();
      } catch (error) { formErrors.textContent = error.message; formErrors.hidden = false; }
      finally { setBusy(form, false); }
    });
    state.main.replaceChildren(heading('Utilisateurs', `Les collaborateurs qui gèrent ${state.site.name} depuis son extranet.`, [refresh]), pageError,
      h('div', { class: 'team-role-guide' }, ['admin', 'editor', 'viewer'].map(role => h('section', {}, h('h2', {}, roleLabels[role]), h('p', {}, roleDescriptions[role])))),
      h('div', { class: 'team-layout' }, h('div', { class: 'team-list-column' }, h('section', { class: 'panel' }, h('div', { class: 'panel-header' }, h('h2', {}, 'Membres de l’équipe'), count), members), h('section', { class: 'panel' }, pending)), form),
      h('p', { class: 'pages-help' }, 'Ces accès sont réservés à l’équipe du site. Les visiteurs créent leur compte séparément sur ', h('code', {}, '/mon-compte'), '.'));
    await load();
  }

  async function sitesList() {
    const grid = h('div', { class: 'sites-grid' });
    for (const site of state.sites) {
      grid.append(h('section', { class: 'panel site-card' }, h('div', { class: 'site-card-top' }, h('span', { class: 'site-avatar', 'aria-hidden': 'true' }, site.name.slice(0, 1).toUpperCase()), site.id === state.site?.id ? h('span', { class: 'status published' }, 'Site actif') : null), h('h2', {}, site.name), h('p', { class: 'site-address' }, `/s/${site.slug}`), site.domain ? h('p', { class: 'hint' }, `Domaine renseigné : ${site.domain}`) : h('p', { class: 'hint' }, 'Vitrine accessible sur son adresse CMS'), h('div', { class: 'site-card-actions' }, link('Gérer le site', adminLink('', site), 'button primary small', 'arrow'), h('a', { class: 'button ghost small', href: `/admin/sites/${encodeURIComponent(site.id)}`, 'aria-label': `Modifier les informations de ${site.name}` }, icon('settings'), 'Informations'))));
    }
    state.main.replaceChildren(heading('Sites clients', 'Un espace distinct pour les contenus et les modules de chaque client.', [link('Créer un site', '/admin/sites/new', 'button primary', 'plus')], 'VOTRE ACTIVITÉ'), grid);
  }

  function extranetAccess(site) {
    const base = `/api/sites/${encodeURIComponent(site.id)}/extranet-access`;
    const panel = h('section', { class: 'panel domain-assistant extranet-access', 'aria-labelledby': 'extranet-access-title' });
    const details = h('div', { class: 'extranet-account', 'aria-live': 'polite' });
    const errorBox = formError();
    const resultBox = h('div', { class: 'extranet-link-result', 'aria-live': 'polite' });
    const passwordArea = h('div');
    const progress = h('p', { class: 'hint', role: 'status' }, 'Chargement de l’accès client…');
    let info = null;
    let busy = false;
    const create = button('Créer un lien d’activation', 'primary', 'plus', createActivation);
    const open = button('Ouvrir l’extranet du client', 'secondary', 'external', openExtranet);
    const refresh = button('Actualiser l’accès', 'ghost small', null, load);
    function toggle(value, message = '') {
      busy = value;
      progress.textContent = message;
      progress.hidden = !message;
      create.disabled = busy || Boolean(info?.account) || !info?.extranet_url;
      open.disabled = busy || !info?.extranet_url;
      refresh.disabled = busy;
      for (const item of passwordArea.querySelectorAll('input,button')) item.disabled = busy;
    }
    function fail(error) { errorBox.textContent = error.message; errorBox.hidden = false; }
    function render() {
      details.replaceChildren(); passwordArea.replaceChildren();
      if (info.account) {
        details.append(h('div', { class: 'domain-connection-heading' }, h('span', { class: 'status published' }, 'Compte principal activé'), h('strong', {}, info.account.name)), h('p', { class: 'hint' }, info.account.email));
        const form = h('form', { class: 'extranet-password-form' });
        const password = field('Nouveau mot de passe du compte principal', 'client-password', { type: 'password', minlength: 12, maxlength: 256, required: true, autocomplete: 'new-password', hint: 'Au moins 12 caractères. Le mot de passe actuel n’est jamais affiché.' });
        const reveal = h('input', { type: 'checkbox', id: 'show-client-password', onChange: () => { password.input.type = reveal.checked ? 'text' : 'password'; } });
        const submit = h('button', { type: 'submit', class: 'button secondary' }, 'Modifier le mot de passe');
        form.append(h('h3', {}, 'Gérer le mot de passe'), password.node, h('label', { class: 'inline-checkbox', for: 'show-client-password' }, reveal, 'Afficher le nouveau mot de passe'), h('p', { class: 'hint' }, 'La modification déconnecte ce compte principal. Les mots de passe des autres membres restent inchangés.'), submit);
        form.addEventListener('submit', async event => {
          event.preventDefault(); if (busy || !form.reportValidity()) return;
          errorBox.hidden = true; toggle(true, 'Modification du mot de passe client…');
          try {
            await api(`${base}/password`, { method: 'POST', body: { password: password.input.value } });
            password.input.value = ''; password.input.type = 'password'; reveal.checked = false;
            toast('Le mot de passe du compte principal est modifié. Ses anciennes sessions sont déconnectées.');
          } catch (error) { fail(error); }
          finally { toggle(false); }
        });
        passwordArea.append(form);
      } else {
        details.append(h('span', { class: 'status' }, 'Accès client à activer'), h('p', { class: 'hint' }, 'Créez un lien privé pour permettre au client de choisir son nom, son adresse e-mail et son mot de passe. Son accès sera limité à ce site.'));
      }
      if (info.account) details.append(link('Gérer les membres de l’équipe', adminLink('users', site), 'text-link team-manage-link', 'arrow'));
      if (info.extranet_url) details.append(h('p', { class: 'extranet-address' }, h('span', {}, 'Adresse de connexion : '), h('code', {}, info.extranet_url)));
      else details.append(h('p', { class: 'notice' }, 'Connectez d’abord un domaine à ce site dans l’assistant Domaines. L’accès extranet sera ensuite disponible sur ce domaine.'));
      create.hidden = Boolean(info.account);
      open.hidden = false;
      toggle(false);
    }
    async function load() {
      if (busy) return;
      errorBox.hidden = true; toggle(true, 'Chargement de l’accès client…');
      try { info = await api(base); render(); }
      catch (error) { fail(error); }
      finally { toggle(false); }
    }
    async function createActivation() {
      if (busy || info?.account || !info?.extranet_url) return;
      errorBox.hidden = true; toggle(true, 'Création du lien privé d’activation…');
      try {
        const activation = await api(base, { method: 'POST' });
        const value = h('input', { type: 'text', readonly: true, value: activation.activation_url, 'aria-label': 'Lien privé d’activation du client', class: 'activation-link' });
        const copy = button('Copier le lien', 'secondary small', null, async () => {
          try { await navigator.clipboard.writeText(activation.activation_url); copy.textContent = 'Lien copié'; }
          catch { value.focus(); value.select(); toast('Sélectionnez et copiez le lien affiché.', true); }
        });
        resultBox.replaceChildren(h('div', { class: 'domain-ai-result' }, h('strong', {}, 'Lien d’activation créé'), h('p', {}, 'Ce lien privé permet d’activer le compte client une seule fois. Transmettez-le uniquement au client concerné.'), value, h('p', { class: 'hint' }, `Valable jusqu’au ${date(activation.expires_at, true)}.`), h('div', { class: 'domain-assistant-actions' }, copy, h('a', { class: 'button ghost small', href: activation.activation_url, target: '_blank', rel: 'noopener noreferrer' }, 'Ouvrir le lien', icon('external')))));
      } catch (error) { fail(error); }
      finally { toggle(false); }
    }
    async function openExtranet() {
      if (busy || !info?.extranet_url) return;
      errorBox.hidden = true; toggle(true, 'Ouverture d’une session limitée à ce client…');
      try {
        const access = await api(`${base}/session`, { method: 'POST' });
        window.location.assign(access.url);
      } catch (error) { fail(error); toggle(false); }
    }
    panel.append(h('div', { class: 'domain-assistant-heading' }, h('div', {}, h('p', { class: 'eyebrow' }, 'ACCÈS DU CLIENT'), h('h2', { id: 'extranet-access-title' }, 'Extranet du site')), refresh), h('p', { class: 'page-subtitle' }, 'Le client gère uniquement les contenus de ce site depuis son propre domaine. Vous conservez la gestion de tous les clients dans ce CMS.'), errorBox, details, h('div', { class: 'domain-assistant-actions' }, create, open), progress, resultBox, passwordArea);
    load();
    return panel;
  }

  function domainAssistant(site) {
    const base = `/api/sites/${encodeURIComponent(site.id)}/domain`;
    const panel = h('section', { class: 'panel domain-assistant', 'aria-labelledby': 'domain-assistant-title' });
    const statusLabels = { queued: 'En attente', running: 'Connexion en cours', waiting_dns: 'En attente des DNS', pending_tls: 'HTTPS en préparation', connected: 'Connecté', failed: 'Connexion à reprendre' };
    const pendingStates = new Set(['queued', 'running', 'waiting_dns', 'pending_tls']);
    let catalog = null;
    let connection = { binding: null, latest_job: null };
    let domains = [];
    let diagnostics = null;
    let action = null;
    let pollTimer = null;
    let pollDeadline = 0;
    let polling = false;
    let pollingPaused = false;
    let suggestionUseButton = null;
    const errorBox = formError();
    const catalogInfo = h('p', { class: 'hint' }, 'Chargement du catalogue de domaines…');
    const assistantInfo = h('div', { class: 'domain-ai-state' });
    const connectionInfo = h('div', { class: 'domain-connection', 'aria-live': 'polite' });
    const diagnosticInfo = h('div', { class: 'domain-diagnostics', 'aria-live': 'polite' });
    const suggestionInfo = h('div', { class: 'domain-suggestion', 'aria-live': 'polite' });
    const activity = h('p', { class: 'domain-activity', role: 'status', hidden: true });
    const selection = h('select', { id: 'assistant-domain', 'aria-describedby': 'assistant-domain-hint', onChange: () => { diagnostics = null; diagnosticInfo.replaceChildren(); renderPlan(); updateControls(); } });
    selection.append(h('option', { value: '' }, 'Chargement des domaines…'));
    const request = field('Votre demande à l’IA (facultatif)', 'domain-request', { textarea: true, rows: 2, maxlength: 500, placeholder: 'Quel domaine utiliser pour la vitrine de ce client ?', hint: 'L’IA locale propose un domaine parmi votre catalogue. Vous choisissez ensuite de le connecter.' });
    const plan = h('div', { class: 'domain-plan' });
    const suggestButton = button('Trouver avec l’IA', 'secondary', 'search', suggest);
    const checkButton = button('Vérifier le domaine', 'secondary', 'check', check);
    const connectButton = button('Connecter le domaine', 'primary', 'external', connect);
    const refreshButton = button('Actualiser l’état', 'ghost small', null, refresh);
    const pending = () => pendingStates.has(connection.latest_job?.status) || connection.binding?.status === 'pending_tls';
    const selected = () => domains.includes(selection.value) ? selection.value : '';
    function fail(error) { errorBox.textContent = error.message || 'Cette opération n’a pas abouti.'; errorBox.hidden = false; }
    function busy(value, message) {
      action = value;
      if (value) errorBox.hidden = true;
      activity.hidden = !message;
      activity.replaceChildren(...(message ? [h('span', { class: 'domain-spinner', 'aria-hidden': 'true' }), message] : []));
      updateControls();
    }
    function updateControls() {
      const domain = selected();
      selection.disabled = Boolean(action) || pending() || domains.length === 0;
      request.input.disabled = Boolean(action);
      suggestButton.disabled = Boolean(action) || pending() || !catalog?.enabled || !catalog?.ready || domains.length === 0;
      checkButton.disabled = Boolean(action) || pending() || !domain;
      connectButton.disabled = Boolean(action) || pending() || !domain || diagnostics?.domain !== domain || diagnostics?.allowed !== true || (connection.binding?.status === 'connected' && connection.binding.domain === domain);
      refreshButton.disabled = Boolean(action) || polling;
      if (suggestionUseButton) suggestionUseButton.disabled = Boolean(action) || pending();
      suggestButton.setAttribute('aria-busy', String(action === 'suggest'));
      checkButton.setAttribute('aria-busy', String(action === 'check'));
      connectButton.setAttribute('aria-busy', String(action === 'connect'));
    }
    function renderPlan() {
      const domain = selected();
      plan.replaceChildren();
      if (!domain) return;
      plan.append(h('strong', {}, `Connexion de ${domain}`), h('p', {}, `Cette action publiera la vitrine de « ${site.name} » sur https://${domain}/ et créera l’accès https://${domain}/extranet vers son administration.`), h('p', { class: 'hint' }, 'Les DNS ne sont pas modifiés automatiquement. La vérification indique les changements éventuellement nécessaires.'));
    }
    function renderConnection() {
      const binding = connection.binding;
      const job = connection.latest_job;
      connectionInfo.replaceChildren();
      if (binding?.status === 'connected') {
        connectionInfo.append(h('div', { class: 'domain-connection-heading' }, h('span', { class: 'status published' }, 'Connecté'), h('strong', {}, binding.domain)), h('div', { class: 'domain-live-links' }, h('a', { href: `https://${binding.domain}/`, target: '_blank', rel: 'noopener', class: 'text-link' }, 'Voir la vitrine', icon('external')), h('a', { href: `https://${binding.domain}/extranet`, target: '_blank', rel: 'noopener', class: 'text-link' }, 'Ouvrir l’extranet', icon('external'))));
      } else if (binding) {
        connectionInfo.append(h('div', { class: 'domain-connection-heading' }, h('span', { class: 'status changed' }, statusLabels[binding.status] || 'Connexion en préparation'), h('strong', {}, binding.domain)));
      } else {
        connectionInfo.append(h('p', { class: 'hint' }, 'Aucun domaine connecté à ce site pour le moment. Sa vitrine reste disponible sur son adresse CMS.'));
      }
      if (job && (job.status !== 'connected' || binding?.status !== 'connected')) {
        connectionInfo.append(h('div', { class: `domain-job${job.status === 'failed' ? ' failed' : ''}` }, h('strong', {}, statusLabels[job.status] || 'État de la connexion'), h('p', {}, job.message || 'La demande de connexion a été enregistrée.'), h('p', { class: 'hint' }, `Domaine : ${job.domain}`), job.next_retry_at && pending() ? h('p', { class: 'hint' }, `Prochaine vérification prévue : ${date(job.next_retry_at, true)}.`) : null));
      }
      if (pending()) connectionInfo.append(h('p', { class: 'hint' }, pollingPaused ? 'Le suivi automatique de cette page est en pause. Utilisez « Actualiser l’état » pour consulter la suite.' : 'L’état se met à jour pendant une minute. Vous pouvez continuer à utiliser le CMS.'));
      updateControls();
    }
    function renderDiagnostics() {
      diagnosticInfo.replaceChildren();
      if (!diagnostics || diagnostics.domain !== selected()) return;
      diagnosticInfo.append(h('div', { class: 'domain-connection-heading' }, h('span', { class: `status${diagnostics.dns_ok ? ' published' : ' changed'}` }, diagnostics.dns_ok ? 'DNS prêts' : 'DNS à configurer'), h('strong', {}, diagnostics.domain)), h('p', {}, diagnostics.message));
      if (!diagnostics.dns_ok && diagnostics.records?.length) {
        const rows = h('tbody');
        for (const record of diagnostics.records) rows.append(h('tr', {}, h('td', {}, record.type), h('td', {}, h('code', {}, record.name)), h('td', {}, h('code', {}, record.value))));
        diagnosticInfo.append(h('p', { class: 'hint' }, 'Dans la zone DNS de votre registraire, appliquez les enregistrements indiqués ci-dessous. Conservez les autres enregistrements, notamment ceux de la messagerie.'), h('div', { class: 'table-scroll' }, h('table', { class: 'domain-records' }, h('thead', {}, h('tr', {}, h('th', { scope: 'col' }, 'Type'), h('th', { scope: 'col' }, 'Nom'), h('th', { scope: 'col' }, 'Valeur'))), rows)), h('p', { class: 'hint' }, 'La connexion restera en attente tant que les DNS ne pointent pas vers ce serveur.'));
      }
      if (Array.isArray(diagnostics.addresses) && diagnostics.addresses.length) diagnosticInfo.append(h('p', { class: 'hint' }, `Adresses actuellement détectées : ${diagnostics.addresses.join(', ')}`));
    }
    function stopPolling() { if (pollTimer !== null) window.clearTimeout(pollTimer); pollTimer = null; }
    function startPolling() {
      stopPolling();
      pollingPaused = false;
      pollDeadline = Date.now() + 60000;
      if (pending()) pollTimer = window.setTimeout(poll, 2500);
      renderConnection();
    }
    async function poll() {
      pollTimer = null;
      if (!panel.isConnected || !pending()) return;
      if (Date.now() >= pollDeadline) { pollingPaused = true; renderConnection(); return; }
      polling = true; updateControls();
      try { connection = await api(base); renderConnection(); }
      catch (error) { pollingPaused = true; fail(error); }
      finally { polling = false; updateControls(); }
      if (pending() && !pollingPaused && panel.isConnected) pollTimer = window.setTimeout(poll, 2500);
    }
    async function refresh() {
      if (action || polling) return;
      if (!catalog) { await load(); return; }
      busy('refresh', 'Actualisation de la connexion…');
      try { connection = await api(base); startPolling(); }
      catch (error) { fail(error); }
      finally { busy(null); }
    }
    async function suggest() {
      if (action || !catalog?.ready || !catalog.enabled) return;
      busy('suggest', 'L’IA locale recherche un domaine dans votre catalogue…');
      suggestionInfo.replaceChildren(); suggestionUseButton = null;
      try {
        const suggestion = await api(`${base}/suggest`, { method: 'POST', body: { request: request.input.value.trim() } });
        if (!domains.includes(suggestion.domain)) throw new Error('Le domaine proposé ne figure pas dans le catalogue importé. Actualisez la page pour réessayer.');
        const use = button('Utiliser ce domaine', 'secondary small', 'check', () => { if (action || pending()) return; selection.value = suggestion.domain; diagnostics = null; diagnosticInfo.replaceChildren(); renderPlan(); updateControls(); use.disabled = true; use.textContent = 'Domaine sélectionné'; });
        suggestionUseButton = use;
        suggestionInfo.replaceChildren(h('div', { class: 'domain-ai-result' }, h('p', { class: 'eyebrow' }, 'PROPOSITION DE L’IA LOCALE'), h('strong', {}, suggestion.domain), h('p', {}, suggestion.explanation), use));
      } catch (error) { fail(error); }
      finally { busy(null); }
    }
    async function check() {
      const domain = selected();
      if (action || !domain || pending()) return;
      busy('check', 'Vérification des DNS et de la disponibilité du domaine…');
      diagnostics = null; diagnosticInfo.replaceChildren();
      try { diagnostics = await api(`${base}/check`, { method: 'POST', body: { domain } }); renderDiagnostics(); }
      catch (error) { fail(error); }
      finally { busy(null); }
    }
    async function connect() {
      const domain = selected();
      if (action || !domain || pending() || diagnostics?.domain !== domain || diagnostics?.allowed !== true) return;
      busy('connect', 'Enregistrement de la demande de connexion…');
      try {
        const job = await api(`${base}/connect`, { method: 'POST', body: { domain } });
        connection.latest_job = job;
        renderConnection();
        startPolling();
      } catch (error) { fail(error); }
      finally { busy(null); }
    }
    async function load() {
      busy('load', 'Chargement de l’assistant et de la connexion…');
      try {
        [catalog, connection] = await Promise.all([api('/api/domain-assistant'), api(base)]);
        const apexDomains = Array.isArray(catalog.domains) ? catalog.domains : [];
        domains = [...new Set(apexDomains.flatMap(domain => [domain, `www.${domain}`]))].sort();
        selection.replaceChildren(h('option', { value: '' }, 'Choisir un domaine'));
        for (const domain of domains) selection.append(h('option', { value: domain }, domain));
        const preferred = site.domain || connection.binding?.domain;
        if (preferred && domains.includes(preferred)) selection.value = preferred;
        const count = apexDomains.length;
        catalogInfo.textContent = `${count} domaine${count > 1 ? 's' : ''} dans la liste de domaines${catalog.synced_at ? ` · Import du ${date(catalog.synced_at, true)}` : ''}.`;
        assistantInfo.replaceChildren(h('span', { class: `status${catalog.ready && catalog.enabled ? ' published' : ''}` }, catalog.enabled && catalog.ready ? 'IA locale disponible' : 'IA locale indisponible'), h('span', {}, catalog.message || (catalog.ready ? 'La recherche s’effectue sur votre VPS.' : 'La sélection manuelle du domaine reste disponible.')));
        renderPlan(); startPolling();
      } catch (error) { fail(error); catalogInfo.textContent = 'Le catalogue de domaines n’a pas pu être chargé.'; selection.replaceChildren(h('option', { value: '' }, 'Catalogue indisponible')); }
      finally { busy(null); }
    }
    panel.append(h('div', { class: 'domain-assistant-heading' }, h('div', {}, h('p', { class: 'eyebrow' }, 'VITRINE ET EXTRANET'), h('h2', { id: 'domain-assistant-title' }, 'Assistant IA · Domaines')), refreshButton), h('p', { class: 'page-subtitle' }, 'Choisissez un domaine de votre liste, vérifiez sa configuration, puis connectez-le à ce site.'), catalogInfo, h('p', { class: 'hint' }, 'Cette liste provient du dernier import ; elle ne se synchronise pas automatiquement.'), assistantInfo, errorBox, connectionInfo, h('div', { class: 'domain-selection' }, h('label', { for: 'assistant-domain' }, 'Domaine à connecter'), selection, h('p', { id: 'assistant-domain-hint', class: 'hint' }, 'Choisissez le domaine principal ou sa version www.')), request.node, h('div', { class: 'domain-assistant-actions' }, suggestButton, checkButton), activity, suggestionInfo, diagnosticInfo, plan, h('div', { class: 'domain-connect-action' }, connectButton));
    window.addEventListener('pagehide', stopPolling, { once: true });
    load();
    return panel;
  }

  async function siteEditor() {
    const fresh = paths[2] === 'new';
    const site = fresh ? { name: '', slug: '', domain: '' } : await api(`/api/sites/${encodeURIComponent(paths[2])}`);
    const form = h('form', { class: 'panel settings-panel' });
    const name = field('Nom du site client', 'name', { value: site.name, required: true, maxlength: 200, placeholder: 'Nom de votre client' });
    const slug = field('Identifiant de la vitrine', 'site-slug', { value: site.slug, required: true, maxlength: 100, pattern: '[a-z0-9]+(-[a-z0-9]+)*', placeholder: 'nom-du-client', hint: 'Adresse de la vitrine : ' + window.location.host + '/s/identifiant. Une modification change son adresse publique.' });
    const domain = field('Domaine (facultatif)', 'domain', { value: site.domain, maxlength: 253, placeholder: 'www.votre-client.fr', hint: fresh ? 'Enregistrez le site pour accéder à l’assistant de connexion de domaine.' : 'Enregistrer cette information ne connecte pas le domaine. Utilisez l’assistant ci-dessous pour vérifier les DNS et lancer la connexion.' });
    let manualSlug = Boolean(site.slug); slug.input.addEventListener('input', () => { manualSlug = true; }); name.input.addEventListener('input', () => { if (!manualSlug) slug.input.value = slugify(name.input.value); });
    const errorBox = formError();
    const save = h('button', { type: 'submit', class: 'button primary' }, fresh ? 'Créer le site client' : 'Enregistrer les informations');
    form.addEventListener('input', () => { state.dirty = true; });
    form.addEventListener('submit', async event => {
      event.preventDefault(); errorBox.hidden = true; setBusy(form, true);
      try {
        const saved = await api(fresh ? '/api/sites' : `/api/sites/${encodeURIComponent(site.id)}`, { method: fresh ? 'POST' : 'PUT', body: { name: name.input.value.trim(), slug: slug.input.value.trim(), domain: domain.input.value.trim() || null } });
        state.dirty = false; window.location.assign(adminLink('', saved));
      } catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; setBusy(form, false); }
    });
    form.append(h('h2', {}, 'Informations du client'), errorBox, name.node, slug.node, domain.node, h('div', { class: 'settings-footer' }, save));
    state.main.replaceChildren(heading(fresh ? 'Nouveau site client' : site.name, 'Chaque site dispose de ses propres pages, articles et réglages.', [link('Retour aux sites', '/admin?view=sites', 'button ghost', 'back')], 'SITES CLIENTS'), h('div', { class: 'site-settings-stack' }, form, fresh ? null : extranetAccess(site), fresh ? null : domainAssistant(site)));
  }

  function auth(setup, activation = '', message = '', invitation = null) {
    document.title = isExtranet ? `${setup ? 'Activer votre accès' : 'Connexion'} · ${state.site?.name || 'Extranet'}` : setup ? 'Créer votre accès · AwningCMS' : 'Connexion · AwningCMS';
    const story = h('aside', { class: 'auth-story' }, h('a', { href: '/', class: 'auth-brand' }, h('span', { class: 'brand-mark', 'aria-hidden': 'true' }, isExtranet ? (state.site?.name || 'A').slice(0, 1) : 'A'), isExtranet ? state.site?.name || 'Votre extranet' : 'AwningCMS'), h('div', { class: 'auth-story-content' }, h('p', { class: 'eyebrow' }, isExtranet ? 'VOTRE SITE, VOTRE ESPACE' : 'VOTRE CMS, VOTRE ESPACE'), h('h1', {}, 'De belles idées.', h('br'), h('span', {}, 'Des sites vivants.')), h('p', {}, isExtranet ? 'Votre espace privé pour préparer vos contenus et faire évoluer votre site.' : 'Un seul espace pour créer les vitrines de vos clients, préparer leurs contenus et les faire évoluer.'), h('div', { class: 'auth-steps' }, h('span', {}, h('b', {}, '01'), isExtranet ? 'Retrouvez votre site' : 'Créez un site client'), h('span', {}, h('b', {}, '02'), isExtranet ? 'Préparez vos pages' : 'Préparez ses pages'), h('span', {}, h('b', {}, '03'), 'Publiez à votre rythme'))), h('p', { class: 'auth-story-footer' }, isExtranet ? 'Votre site · Espace client' : 'AwningCMS · Espace d’administration'));
    const form = h('form', { class: 'auth-form' });
    const email = field('Adresse e-mail', 'email', { type: 'email', required: true, autocomplete: 'username', maxlength: 254, placeholder: 'vous@entreprise.fr', value: invitation?.email || '' });
    const password = field('Mot de passe', 'password', { type: 'password', required: true, minlength: setup ? 12 : undefined, maxlength: 256, autocomplete: setup ? 'new-password' : 'current-password', hint: setup ? 'Au moins 12 caractères. Utilisez une phrase de passe personnelle.' : undefined });
    const name = setup ? field('Votre nom', 'name', { required: true, maxlength: 100, autocomplete: 'name', placeholder: 'Prénom et nom', value: invitation?.name || '' }) : null;
    const siteName = setup && !isExtranet && !invitation ? field('Nom du site', 'site-name', { required: true, maxlength: 100, autocomplete: 'organization', placeholder: 'Le nom de votre entreprise' }) : null;
    const token = setup ? field(isExtranet ? 'Code d’activation de votre accès' : 'Clé d’activation', 'activation', { value: activation, type: 'password', required: true, autocomplete: 'off', hint: isExtranet ? 'Ce code est prérempli par le lien privé transmis par votre administrateur.' : 'La clé privée remise lors de l’installation du CMS.' }) : null;
    const errorBox = formError(message);
    const submit = h('button', { type: 'submit', class: 'button primary full' }, setup ? isExtranet ? 'Activer mon accès client' : 'Créer mon accès administrateur' : 'Se connecter', icon('arrow'));
    form.append(h('p', { class: 'eyebrow' }, setup ? invitation ? 'REJOINDRE L’ÉQUIPE' : 'PREMIÈRE CONNEXION' : 'HEUREUX DE VOUS RETROUVER'), h('h2', {}, setup ? 'Bienvenue chez vous.' : 'Votre espace vous attend.'), h('p', { class: 'auth-description' }, setup ? invitation ? `Vous rejoignez ${state.site?.name || 'ce site'} avec le rôle ${roleLabels[invitation.role] || invitation.role}. ${roleDescriptions[invitation.role] || ''}` : isExtranet ? 'Choisissez vos identifiants pour gérer les contenus de votre site.' : 'Créez votre compte administrateur pour gérer les sites de vos clients.' : isExtranet ? 'Connectez-vous pour retrouver les contenus de votre site.' : 'Connectez-vous pour retrouver vos sites et vos contenus.'), errorBox, ...(setup ? [invitation ? null : token.node, name.node, siteName ? siteName.node : null] : []), email.node, password.node, submit, h('p', { class: 'auth-footnote' }, icon('lock'), setup ? 'Cette activation n’est disponible qu’une seule fois.' : isExtranet ? 'Cet accès est limité à votre site.' : 'Accès réservé aux administrateurs du CMS.'), link('Retour à la vitrine', '/', 'auth-back', 'back'));
    if (invitation) { email.input.readOnly = true; email.input.setAttribute('aria-readonly', 'true'); }
    form.addEventListener('submit', async event => {
      event.preventDefault(); errorBox.hidden = true; setBusy(form, true);
      try {
        await api(invitation ? '/api/team-invitation' : setup ? isExtranet ? '/api/activate' : '/api/setup' : '/api/auth/login', { method: 'POST', body: invitation ? { token: token.input.value.trim(), name: name.input.value.trim(), password: password.input.value } : setup ? { token: token.input.value.trim(), name: name.input.value.trim(), email: email.input.value.trim(), password: password.input.value, ...(siteName ? { site_name: siteName.input.value.trim() } : {}) } : { email: email.input.value.trim(), password: password.input.value } });
        if (setup) token.input.value = '';
        window.location.replace(authLink('/admin'));
      } catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; setBusy(form, false); }
    });
    app.replaceChildren(h('main', { id: 'main', class: 'auth-page' }, story, h('section', { class: 'auth-form-area' }, form)));
  }

  async function boot() {
    const fragment = new URLSearchParams(window.location.hash.slice(1).replace(/^\?/, ''));
    let activation = fragment.get('activation') || (isExtranet ? url.searchParams.get('activation') || '' : '');
    let access = isExtranet ? fragment.get('access') || '' : '';
    let invitation = isExtranet ? fragment.get('invitation') || '' : '';
    if (window.location.hash || url.searchParams.has('activation') || url.searchParams.has('access')) {
      const clean = new URL(window.location.href);
      clean.hash = '';
      clean.searchParams.delete('activation'); clean.searchParams.delete('access');
      if (isExtranet) clean.searchParams.delete('site');
      window.history.replaceState(null, '', clean.pathname + clean.search);
    }
    try {
      if (access) { await api('/api/access', { method: 'POST', body: { token: access } }); access = ''; }
      const bootstrap = await api('/api/bootstrap');
      if (isExtranet && bootstrap.site) state.site = bootstrap.site;
      if (isExtranet && invitation) {
        try { const details = await api('/api/team-invitation/inspect', { method: 'POST', body: { token: invitation } }); auth(true, invitation, '', details); }
        catch (error) { auth(false, '', error.message); }
        invitation = ''; return;
      }
      if (isExtranet && activation) { window.history.replaceState(null, '', authLink('/admin/setup')); auth(true, activation); activation = ''; return; }
      if (!bootstrap.initialized) { window.history.replaceState(null, '', authLink('/admin/setup')); auth(true, activation); return; }
      try { state.user = await api('/api/auth/me'); }
      catch (error) { if (error.status === 401) { window.history.replaceState(null, '', authLink('/admin/login')); auth(false, '', isExtranet && bootstrap.activation_required ? 'Votre accès client n’est pas encore activé. Demandez votre lien privé d’activation à votre administrateur.' : ''); return; } throw error; }
      if (paths[1] === 'login' || paths[1] === 'setup') { window.location.replace(authLink('/admin')); return; }
      state.sites = await api('/api/sites');
      const requested = requestedSite;
      state.site = state.sites.find(site => site.id === requested) || state.sites[0] || null;
      if (requested && !state.sites.some(site => site.id === requested)) throw new Error('Ce site client est introuvable. Revenez à la liste des sites.');
      if (state.site) state.settings = await api(endpoint('/settings'));
      shell();
      if (isExtranet && (paths[1] === 'sites' || url.searchParams.get('view') === 'sites')) { window.location.replace(adminBase); return; }
      if (!isExtranet && paths[1] === 'sites' && paths[2]) await siteEditor();
      else if (!isExtranet && (url.searchParams.get('view') === 'sites' || !state.site)) await sitesList();
      else if (paths[1] === 'settings') await settings();
      else if (paths[1] === 'contents' && paths[2]) await editor();
      else if (url.searchParams.get('view') === 'pages' || paths[1] === 'pages') await listContents('page');
      else if (url.searchParams.get('view') === 'template') await templateView();
      else if (url.searchParams.get('view') === 'builder') await visualBuilder();
      else if (url.searchParams.get('view') === 'articles') await listContents('article');
      else if (url.searchParams.get('view') === 'modules') await modules();
      else if (url.searchParams.get('view') === 'users') await teamUsers();
      else await dashboard();
      state.main.removeAttribute('aria-busy');
    } catch (error) {
      if (!state.main) { state.main = h('main', { id: 'main', class: 'main-content standalone-error' }); app.replaceChildren(state.main); }
      showError(error);
    }
  }
  window.addEventListener('beforeunload', event => { if (state.dirty) { event.preventDefault(); event.returnValue = ''; } });
  boot();
})();
