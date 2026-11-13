'use strict';

window.CMSBuilder = (() => {
  const labels = { heading: 'Titre', text: 'Éditeur de texte', image: 'Image', button: 'Bouton', divider: 'Séparateur', spacer: 'Espaceur', hero: 'Bannière', columns: 'Colonnes', cta: 'Appel à l’action' };
  const descriptions = { heading: 'Un titre qui donne le ton.', button: 'Un lien pour guider les visiteurs.', divider: 'Une ligne pour rythmer la page.', spacer: 'Un espace pour laisser respirer.', hero: 'Un titre, une image et un premier message.', text: 'Présentez votre histoire ou votre activité.', image: 'Une photographie avec sa légende.', columns: 'Deux à quatre idées côte à côte.', cta: 'Invitez vos visiteurs à passer à l’action.' };
  const drawings = { heading: 'M4 4h16M12 4v16M8 20h8', text: 'M4 5h16M4 10h16M4 15h16M4 20h10', image: 'M3 3h18v18H3zM3 17l6-6 5 5 3-3 4 4M15 7h.01', button: 'M3 7h18v10H3zM8 12h8', divider: 'M3 12h18', spacer: 'M4 4h16M4 20h16M12 6v12m-3-9 3-3 3 3m-6 6 3 3 3-3', hero: 'M3 3h18v18H3zM7 8h10M7 12h7M7 16h4', columns: 'M3 3h7v18H3zM14 3h7v18h-7z', cta: 'M3 3h18v18H3zM7 8h10M8 14h8v3H8z', search: 'M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14m5 12 6 6', grid: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z', tablet: 'M4 2h16v20H4zM11 18h2', layers: 'm3 7 9-5 9 5-9 5-9-5m0 5 9 5 9-5m-18 5 9 5 9-5', close: 'm6 6 12 12M18 6 6 18', palette: 'M12 3a9 9 0 1 0 0 18h2a2 2 0 0 0 0-4h-1a2 2 0 0 1 0-4h5a3 3 0 0 0 3-3c0-4-4-7-9-7M7 9h.01M10 6h.01M15 6h.01M18 9h.01', edit: 'm15 4 5 5M3 21l5-1L21 7l-5-5L3 15z', back: 'M19 12H5m6-6-6 6 6 6', plus: 'M12 5v14M5 12h14', up: 'm6 15 6-6 6 6', down: 'm6 9 6 6 6-6', undo: 'M4 10h10a6 6 0 0 1 0 12M4 10l5-5M4 10l5 5', redo: 'M20 10H10a6 6 0 0 0 0 12M20 10l-5-5M20 10l-5 5', desktop: 'M3 3h18v14H3zM12 17v4M7 21h10', mobile: 'M7 2h10v20H7zM11 18h2', copy: 'M8 7h12v14H8zM4 17V3h12', trash: 'M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7', grip: 'M9 5h.01M15 5h.01M9 12h.01M15 12h.01M9 19h.01M15 19h.01', check: 'm5 12 4 4L19 6', settings: 'M4 6h16M4 12h16M4 18h16M8 3v6M16 9v6M10 15v6', layout: 'M3 3h18v18H3zM3 8h18M8 8v13', eye: 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6', arrow: 'M4 12h16m-6-6 6 6-6 6', reset: 'M3 10a9 9 0 1 1 2 8M3 4v6h6' };
  function h(tag, attributes = {}, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attributes)) {
      if (value == null || value === false) continue;
      if (key.startsWith('on')) node.addEventListener(key.slice(2).toLowerCase(), value);
      else if (key === 'class') node.className = value;
      else if (['value', 'checked', 'disabled'].includes(key)) node[key] = value;
      else node.setAttribute(key, value === true ? '' : String(value));
    }
    for (const child of children.flat(Infinity)) if (child != null && child !== false) node.append(child instanceof Node ? child : document.createTextNode(String(child)));
    return node;
  }
  function icon(name) {
    const node = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    node.setAttribute('viewBox', '0 0 24 24'); node.setAttribute('class', 'builder-icon'); node.setAttribute('aria-hidden', 'true');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path'); path.setAttribute('d', drawings[name] || drawings.layout); node.append(path); return node;
  }
  function button(text, action, symbol, className = '') { return h('button', { type: 'button', class: `builder-button ${className}`, onClick: action }, symbol ? icon(symbol) : null, text); }
  const clone = value => JSON.parse(JSON.stringify(value));
  const uuid = () => crypto.randomUUID();
  function preset(type) {
    const common = { id: uuid(), type, align: type === 'hero' || type === 'cta' ? 'center' : 'left', spacing: 'large', theme: type === 'cta' ? 'sage' : 'paper' };
    if (type === 'heading') return { ...common, spacing: 'small', title: 'Un titre qui vous ressemble' };
    if (type === 'button') return { ...common, spacing: 'small', button: { label: 'En savoir plus', href: '/a-propos' } };
    if (type === 'divider') return { ...common, spacing: 'small' };
    if (type === 'spacer') return { ...common, spacing: 'small', height: 64 };
    if (type === 'hero') return { ...common, eyebrow: 'BIENVENUE', title: 'Une nouvelle histoire commence ici.', text: 'Découvrez notre univers, nos idées et ce qui nous anime.', image_url: null, image_alt: '', button: { label: 'En savoir plus', href: '/a-propos' } };
    if (type === 'text') return { ...common, title: 'Un peu plus sur nous', text: 'Présentez votre activité et partagez ce qui la rend unique.' };
    if (type === 'image') return { ...common, url: '/assets/site-construction.webp', alt: 'Illustration de construction du site', caption: '' };
    if (type === 'columns') return { ...common, title: 'Ce qui nous guide', items: [{ title: 'Notre savoir-faire', text: 'Présentez un premier point fort de votre activité.' }, { title: 'Nos engagements', text: 'Partagez ce qui compte pour vous et vos clients.' }] };
    return { ...common, title: 'Faisons connaissance.', text: 'Retrouvez notre histoire et les dernières nouvelles du site.', button: { label: 'Découvrir notre histoire', href: '/a-propos' } };
  }

  async function mount(config) {
    const target = config.target;
    const templateMode = target === 'template';
    const base = target === 'home' || templateMode ? config.endpoint(`/design/${target}`) : config.endpoint(`/contents/${encodeURIComponent(target)}/layout`);
    const publishPath = target === 'home' || templateMode ? `${base}/publish` : config.endpoint(`/contents/${encodeURIComponent(target)}/publish`);
    const editable = templateMode ? config.canTemplateEdit : config.canEdit;
    const [initial, structure] = await Promise.all([config.api(base), config.api(config.endpoint('/structure'))]);
    let meta = initial;
    let doc = clone(initial.document);
    let baseline = JSON.stringify(doc);
    let selected = templateMode ? 'header' : null;
    let inspectorTab = 'content';
    let canvasOnly = false;
    let inlineEdit = null;
    let suppressWidgetClick = false;
    let device = 'desktop';
    let busy = false;
    let savedInEditor = false;
    let previewTimer = null;
    let previewVersion = 0;
    let lastMerge = '';
    let mergeAt = 0;
    const history = [];
    const future = [];
    const main = config.root;
    main.classList.add('is-builder'); document.documentElement.classList.add('builder-open');
    document.title = `Éditeur visuel · ${config.title} · ${config.site.name}`;
    const saveState = h('span', { class: 'builder-save-state', role: 'status', 'aria-live': 'polite' });
    const alert = h('div', { class: 'builder-alert', role: 'alert', hidden: true });
    const announcement = h('span', { class: 'sr-only', role: 'status', 'aria-live': 'polite' });
    const layers = h('div', { class: 'builder-layers' });
    const properties = h('div', { class: 'builder-properties', hidden: !templateMode });
    const propsBody = h('div', { class: 'builder-properties-body' });
    const inspectorTitle = h('h2', {}, templateMode ? 'Modèle du site' : 'Modifier l’élément');
    const inspectorTabs = h('div', { class: 'builder-inspector-tabs', role: 'tablist', 'aria-label': 'Réglages de l’élément', hidden: templateMode });
    for (const [key, label, symbol] of [['content', 'Contenu', 'edit'], ['style', 'Style', 'palette'], ['advanced', 'Avancé', 'settings']]) {
      const tab = button(label, () => { inspectorTab = key; renderProperties(); }, symbol);
      tab.dataset.inspectorTab = key; tab.setAttribute('role', 'tab'); tab.setAttribute('aria-selected', String(key === 'content')); inspectorTabs.append(tab);
    }
    const frame = h('iframe', { title: `Aperçu de ${config.title}`, class: 'builder-frame', sandbox: 'allow-same-origin', referrerpolicy: 'same-origin', tabindex: '-1' });
    const canvas = h('div', { class: 'builder-canvas' }, frame);
    const previewState = h('div', { class: 'builder-preview-state', role: 'status', 'aria-live': 'polite' }, 'Préparation de l’aperçu…');
    const stage = h('div', { class: 'builder-stage' }, previewState, canvas);
    const sectionPanel = h('aside', { class: 'builder-section-panel', 'aria-label': 'Éléments et réglages' });
    const workspace = h('div', { class: 'builder-workspace', 'data-panel': 'preview' });
    const library = h('div', { class: 'builder-library', hidden: templateMode });
    const widgetGrid = h('div', { class: 'builder-widget-grid' });
    const search = h('input', { type: 'search', placeholder: 'Rechercher un élément…', 'aria-label': 'Rechercher un élément', onInput: renderLibrary });
    library.append(h('div', { class: 'builder-panel-heading' }, h('div', { class: 'builder-panel-kicker' }, h('span', { class: 'builder-eyebrow' }, 'VOTRE BOÎTE À OUTILS'), h('span', { class: 'builder-count' }, '9')), h('h2', {}, 'Éléments'), h('p', {}, editable ? 'Glissez un élément sur la page ou cliquez pour l’ajouter.' : 'Sélectionnez un élément sur la page pour consulter ses réglages.')), h('div', { class: 'builder-widget-search' }, icon('search'), search), h('h3', { class: 'builder-library-label' }, 'Les essentiels'), widgetGrid);
    library.append(h('p', { class: 'builder-side-note' }, 'Les menus sont communs à toutes les pages.', config.templateHref ? h('a', { href: config.templateHref }, config.canTemplateEdit ? 'Modifier l’en-tête et le pied de page' : 'Consulter le modèle', icon('arrow')) : null));
    const structurePanel = h('aside', { class: 'builder-structure-panel', hidden: true, 'aria-label': 'Structure de la page' });
    const structureToggle = button('Structure', () => toggleStructure(), 'layers', 'builder-structure-toggle');
    structureToggle.setAttribute('aria-expanded', 'false');
    structurePanel.append(h('div', { class: 'builder-structure-heading' }, h('div', {}, icon('layers'), h('h2', {}, 'Structure')), button('Fermer la structure', () => toggleStructure(false), 'close', 'icon-only')), h('p', { class: 'builder-structure-hint' }, templateMode ? 'Les parties communes de votre site.' : 'Réorganisez les éléments de haut en bas.'), layers);
    const contextToolbar = h('div', { class: 'builder-context-toolbar', hidden: true, 'aria-label': 'Actions de la section sélectionnée' });
    const undo = button('Annuler', () => travel(history, future), 'undo', 'icon-only');
    const redo = button('Rétablir', () => travel(future, history), 'redo', 'icon-only');
    undo.title = 'Annuler la dernière modification'; redo.title = 'Rétablir la modification';
    const save = button('Enregistrer le brouillon', saveDraft, null, 'secondary');
    const publish = button('Publier', publishDocument, 'arrow', 'primary');
    const back = h('a', { class: 'builder-back', href: config.backHref, 'aria-label': 'Retour aux pages' }, icon('back'));
    const controls = h('div', { class: 'builder-device-controls', role: 'group', 'aria-label': 'Format de l’aperçu' });
    for (const [key, label, symbol] of [['desktop', 'Ordinateur', 'desktop'], ['tablet', 'Tablette', 'tablet'], ['mobile', 'Mobile', 'mobile']]) {
      const control = button(label, () => setDevice(key), symbol, 'icon-only');
      control.dataset.device = key; control.setAttribute('aria-label', label); control.title = label; control.setAttribute('aria-pressed', String(device === key)); controls.append(control);
    }
    const expand = button('Agrandir l’aperçu', () => { canvasOnly = !canvasOnly; workspace.classList.toggle('is-canvas-only', canvasOnly); expand.setAttribute('aria-pressed', String(canvasOnly)); expand.setAttribute('aria-label', canvasOnly ? 'Afficher les panneaux' : 'Agrandir l’aperçu'); expand.title = canvasOnly ? 'Afficher les panneaux' : 'Agrandir l’aperçu'; resizeFrame(); }, 'eye', 'icon-only');
    expand.setAttribute('aria-pressed', 'false'); expand.title = 'Agrandir l’aperçu';
    const top = h('header', { class: 'builder-topbar' }, h('div', { class: 'builder-topbar-title' }, back, h('span', { class: 'builder-brand', 'aria-hidden': 'true' }, 'C'), h('div', {}, h('span', { class: 'builder-site-name' }, config.site.name), h('h1', {}, config.title))), controls,
      h('div', { class: 'builder-topbar-actions' }, editable ? h('div', { class: 'builder-history' }, undo, redo) : h('span', { class: 'builder-readonly' }, 'Lecture seule'), expand, structureToggle));
    const mobileTabs = h('div', { class: 'builder-mobile-tabs', role: 'group', 'aria-label': 'Panneau de l’éditeur' });
    for (const [key, text, symbol] of [['sections', templateMode ? 'Modèle' : 'Éléments', 'grid'], ['preview', 'Aperçu', 'eye'], ['properties', 'Réglages', 'settings']]) {
      const tab = button(text, () => showPanel(key), symbol); tab.dataset.panel = key; tab.setAttribute('aria-pressed', String(key === 'preview')); mobileTabs.append(tab);
    }
    properties.append(h('div', { class: 'builder-inspector-heading' }, !templateMode ? button('Revenir aux éléments', () => { library.hidden = false; properties.hidden = true; showPanel('sections'); }, 'grid', 'icon-only') : icon('layout'), inspectorTitle), inspectorTabs, propsBody);
    if (templateMode) properties.prepend(h('div', { class: 'builder-template-switch' }, button('En-tête', () => select('header'), 'layout'), button('Pied de page', () => select('footer'), 'layout')));
    sectionPanel.append(library, properties);
    const deviceLabel = h('span', { class: 'builder-device-label' }, 'Ordinateur · 1024 px');
    const bottom = h('footer', { class: 'builder-bottom-bar' }, h('div', { class: 'builder-bottom-status' }, h('span', { class: 'builder-status-dot', 'aria-hidden': 'true' }), saveState), h('span', { class: 'builder-bottom-note' }, editable ? 'Vos changements restent privés jusqu’à leur publication.' : 'Aperçu en lecture seule'), h('div', { class: 'builder-publish-actions' }, editable ? save : null, editable ? publish : null));
    workspace.append(sectionPanel, h('section', { class: 'builder-preview-panel', 'aria-label': 'Aperçu de la page' }, h('div', { class: 'builder-canvas-label' }, deviceLabel, h('span', {}, editable ? 'Double-cliquez sur un texte pour le modifier' : 'Aperçu en lecture seule')), stage), structurePanel);
    main.replaceChildren(top, alert, mobileTabs, workspace, bottom, contextToolbar, announcement);

    function dirty() { return JSON.stringify(doc) !== baseline; }
    function updateState() {
      const changed = dirty(); config.onDirty(changed);
      saveState.textContent = busy ? 'En cours…' : changed ? 'Modifications non enregistrées' : meta.has_unpublished_changes ? 'Brouillon enregistré' : meta.published_at ? 'Version publiée' : 'Prêt à personnaliser';
      saveState.classList.toggle('is-dirty', changed);
      undo.disabled = busy || !history.length; redo.disabled = busy || !future.length;
      save.disabled = busy || (!changed && savedInEditor);
      publish.disabled = busy || (!changed && !meta.has_unpublished_changes && meta.published_revision != null);
      for (const input of main.querySelectorAll('.builder-properties-body input, .builder-properties-body textarea, .builder-properties-body select, .builder-properties-body button, .builder-add-section, .builder-layer-tools button, .builder-drag-handle, .builder-context-toolbar button')) if (input.dataset.viewControl !== 'true') input.disabled = busy || !editable || input.dataset.unavailable === 'true';
    }
    function showPanel(key) {
      workspace.dataset.panel = key === 'preview' ? 'preview' : 'sidebar';
      if (key === 'sections' && !templateMode) { library.hidden = false; properties.hidden = true; }
      if (key === 'properties' || templateMode && key === 'sections') { library.hidden = true; properties.hidden = false; }
      for (const tab of mobileTabs.children) tab.setAttribute('aria-pressed', String(tab.dataset.panel === key));
      resizeFrame();
    }
    function toggleStructure(force) {
      structurePanel.hidden = force == null ? !structurePanel.hidden : !force;
      structureToggle.setAttribute('aria-expanded', String(!structurePanel.hidden));
    }
    function setDevice(key) {
      device = key; canvas.dataset.device = key;
      for (const item of controls.children) item.setAttribute('aria-pressed', String(item.dataset.device === key));
      deviceLabel.textContent = `${{ desktop: 'Ordinateur', tablet: 'Tablette', mobile: 'Mobile' }[key]} · ${{ desktop: 1024, tablet: 820, mobile: 390 }[key]} px`;
      if (selected && inspectorTab !== 'content') renderProperties(); resizeFrame();
    }
    function renderLibrary() {
      const query = search.value.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();
      widgetGrid.replaceChildren();
      for (const type of Object.keys(labels)) {
        if (query && !`${labels[type]} ${descriptions[type]}`.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().includes(query)) continue;
        const tile = button('', () => { if (!suppressWidgetClick) addSection(type); }, type, 'builder-add-section');
        tile.append(h('strong', {}, labels[type])); tile.title = descriptions[type]; tile.setAttribute('aria-label', `Ajouter ${labels[type]}`); tile.disabled = !editable; attachWidgetDrag(tile, type); widgetGrid.append(tile);
      }
      if (!widgetGrid.childElementCount) widgetGrid.append(h('p', { class: 'builder-no-widgets' }, 'Aucun élément trouvé. Essayez un autre mot.'));
    }
    function fail(error) {
      alert.replaceChildren(h('div', {}, h('strong', {}, error.status === 409 ? 'Une autre version a été enregistrée.' : error.status === 401 ? 'Votre session a expiré.' : 'Cette modification n’a pas abouti.'), h('p', {}, error.status === 409 ? 'Vos changements restent dans cet éditeur. Exportez votre brouillon avant de recharger la version du serveur.' : error.status === 401 ? 'Vos changements restent dans cet éditeur. Reconnectez-vous dans un autre onglet, puis réessayez.' : error.message || 'Réessayez dans un instant.')));
      if (error.status === 409) alert.append(button('Exporter mon brouillon', exportDraft, null, 'secondary'), button('Recharger', () => { if (!dirty() || window.confirm('Recharger la version enregistrée ? Les changements non enregistrés seront perdus. Exportez votre brouillon pour les conserver.')) { config.onDirty(false); window.location.reload(); } }, null, 'secondary'));
      if (error.status === 401) alert.append(button('Exporter mon brouillon', exportDraft, null, 'secondary'), h('a', { href: config.authHref, target: '_blank', rel: 'noopener', class: 'builder-button secondary' }, 'Se reconnecter'));
      alert.hidden = false;
    }
    function exportDraft() {
      const objectUrl = URL.createObjectURL(new Blob([JSON.stringify(doc, null, 2)], { type: 'application/json' }));
      const link = h('a', { href: objectUrl, download: `brouillon-${target}.json` }); link.click(); window.setTimeout(() => URL.revokeObjectURL(objectUrl), 1000);
    }
    function change(mutator, options = {}) {
      if (!editable || busy) return;
      const before = clone(doc); mutator(doc);
      if (JSON.stringify(before) === JSON.stringify(doc)) return;
      const now = Date.now();
      if (!options.merge || options.merge !== lastMerge || now - mergeAt > 1200) { history.push(before); if (history.length > 60) history.shift(); }
      lastMerge = options.merge || ''; mergeAt = now; future.length = 0;
      alert.hidden = true; renderLayers(); if (options.properties) renderProperties(); updateState(); queuePreview();
    }
    function travel(from, to) {
      if (!from.length || busy || !editable) return;
      to.push(clone(doc)); doc = from.pop(); lastMerge = '';
      if (!templateMode && !doc.sections.some(item => item.id === selected)) selected = doc.sections[0]?.id || null;
      renderLayers(); renderProperties(); updateState(); queuePreview(0);
    }
    function select(id, fromCanvas = false) {
      if (inlineEdit) inlineEdit.finish(false);
      selected = id; lastMerge = ''; library.hidden = true; properties.hidden = false; renderLayers(); renderProperties(); markSelection();
      if (fromCanvas && window.matchMedia('(max-width: 900px)').matches) showPanel('properties');
    }
    function addSection(type, position = doc.sections.length) {
      if (doc.sections.length >= 50) { fail(new Error('Cette page contient déjà 50 sections. Supprimez une section avant d’en ajouter une.')); return; }
      const section = preset(type); selected = section.id; inspectorTab = 'content'; library.hidden = true; properties.hidden = false;
      change(value => { value.sections.splice(position, 0, section); }, { properties: true }); showPanel('properties'); announcement.textContent = `${labels[type]} ajoutée.`;
    }
    function reorder(id, direction) {
      const index = doc.sections.findIndex(section => section.id === id);
      const next = index + direction;
      if (next < 0 || next >= doc.sections.length) return;
      change(value => { const [section] = value.sections.splice(index, 1); value.sections.splice(next, 0, section); }); announcement.textContent = `Section déplacée en position ${next + 1}.`;
    }
    function duplicateSelected() {
      const section = currentSection(); if (!section || !editable || busy) return;
      if (doc.sections.length >= 50) { fail(new Error('Le maximum de 50 éléments est atteint.')); return; }
      const copy = clone(section); copy.id = uuid(); selected = copy.id;
      change(document => { document.sections.splice(document.sections.findIndex(item => item.id === section.id) + 1, 0, copy); }, { properties: true });
    }
    function deleteSelected() {
      const section = currentSection(); if (!section || !editable || busy) return;
      change(document => { const index = document.sections.findIndex(item => item.id === section.id); document.sections.splice(index, 1); selected = document.sections[Math.min(index, document.sections.length - 1)]?.id || null; }, { properties: true });
    }
    function positionContextToolbar() {
      try {
        const root = frame.contentDocument;
        const element = root && [...root.querySelectorAll(templateMode ? '[data-template-area]' : '[data-section-id]')].find(item => templateMode ? item.dataset.templateArea === selected : item.dataset.sectionId === selected);
        if (!element || !selected || !stage.offsetWidth || canvasOnly) { contextToolbar.hidden = true; return; }
        const rect = element.getBoundingClientRect();
        const frameRect = frame.getBoundingClientRect(); const stageRect = stage.getBoundingClientRect();
        const scale = frameRect.width / root.documentElement.clientWidth;
        const top = frameRect.top + rect.top * scale;
        const bottom = frameRect.top + rect.bottom * scale;
        if (rect.height === 0 || bottom < stageRect.top + 20 || top > stageRect.bottom - 20) { contextToolbar.hidden = true; return; }
        const name = templateMode ? selected === 'header' ? 'En-tête' : 'Pied de page' : labels[currentSection()?.type] || 'Élément';
        contextToolbar.replaceChildren(h('span', {}, inlineEdit ? 'Entrée : valider · Échap : annuler' : name));
        if (editable && !templateMode && !inlineEdit) {
          const index = doc.sections.findIndex(item => item.id === selected);
          const up = button('Monter', () => reorder(selected, -1), 'up', 'icon-only'); up.disabled = index === 0;
          const down = button('Descendre', () => reorder(selected, 1), 'down', 'icon-only'); down.disabled = index === doc.sections.length - 1;
          contextToolbar.append(up, down, button('Dupliquer', duplicateSelected, 'copy', 'icon-only'), button('Supprimer', deleteSelected, 'trash', 'icon-only'));
        }
        contextToolbar.hidden = false;
        const left = frameRect.left + rect.left * scale;
        contextToolbar.style.left = `${Math.max(stageRect.left + 8, Math.min(left, stageRect.right - contextToolbar.offsetWidth - 8))}px`;
        contextToolbar.style.top = `${Math.max(stageRect.top + 8, top - 34)}px`;
      } catch { contextToolbar.hidden = true; }
    }
    stage.addEventListener('scroll', positionContextToolbar, { passive: true });
    function dropPosition(clientX, clientY) {
      const visible = stage.getBoundingClientRect(); const frameRect = frame.getBoundingClientRect();
      if (clientX < Math.max(frameRect.left, visible.left) || clientX > Math.min(frameRect.right, visible.right) || clientY < visible.top || clientY > visible.bottom) return null;
      const document = frame.contentDocument; if (!document?.body) return null;
      const scale = frameRect.width / document.documentElement.clientWidth;
      const candidates = [...document.querySelectorAll('[data-section-id]')].map(node => ({ node, rect: node.getBoundingClientRect() })).filter(item => item.rect.height > 0);
      let candidate = candidates.find(item => clientY < frameRect.top + (item.rect.top + item.rect.height / 2) * scale);
      let after = false;
      if (!candidate) { candidate = candidates.at(-1); after = true; }
      if (!candidate) return { index: 0, top: Math.max(visible.top + 20, frameRect.top + 100), left: frameRect.left, width: frameRect.width };
      const index = doc.sections.findIndex(item => item.id === candidate.node.dataset.sectionId);
      return { index: index < 0 ? doc.sections.length : index + (after ? 1 : 0), top: Math.min(visible.bottom - 5, Math.max(visible.top + 5, frameRect.top + (after ? candidate.rect.bottom : candidate.rect.top) * scale)), left: frameRect.left + candidate.rect.left * scale, width: candidate.rect.width * scale };
    }
    function attachWidgetDrag(tile, type) {
      tile.addEventListener('pointerdown', event => {
        if (!editable || busy || event.button !== 0) return;
        const start = { x: event.clientX, y: event.clientY }; let dragging = false; let destination = null;
        const ghost = h('div', { class: 'builder-widget-ghost', hidden: true }, icon(type), labels[type]);
        const line = h('div', { class: 'builder-drop-indicator', hidden: true }, h('span', {}, 'Insérer ici'));
        main.append(ghost, line); tile.setPointerCapture(event.pointerId);
        const move = moving => {
          if (!dragging && Math.hypot(moving.clientX - start.x, moving.clientY - start.y) < 7) return;
          dragging = true; moving.preventDefault(); ghost.hidden = false;
          ghost.style.left = `${moving.clientX + 14}px`; ghost.style.top = `${moving.clientY + 12}px`;
          destination = dropPosition(moving.clientX, moving.clientY);
          line.hidden = !destination;
          if (destination) { line.style.left = `${destination.left}px`; line.style.top = `${destination.top}px`; line.style.width = `${destination.width}px`; }
          const bounds = stage.getBoundingClientRect();
          if (destination && moving.clientY < bounds.top + 55) stage.scrollTop -= 18;
          if (destination && moving.clientY > bounds.bottom - 55) stage.scrollTop += 18;
        };
        const cleanup = () => { ghost.remove(); line.remove(); tile.removeEventListener('pointermove', move); tile.removeEventListener('pointerup', finish); tile.removeEventListener('pointercancel', cancel); document.removeEventListener('keydown', escape); if (tile.hasPointerCapture(event.pointerId)) tile.releasePointerCapture(event.pointerId); };
        const cancel = () => { if (dragging) { suppressWidgetClick = true; window.setTimeout(() => { suppressWidgetClick = false; }, 150); } cleanup(); };
        const escape = key => { if (key.key === 'Escape') { key.preventDefault(); cancel(); } };
        const finish = released => {
          if (dragging) { released.preventDefault(); suppressWidgetClick = true; window.setTimeout(() => { suppressWidgetClick = false; }, 150); }
          cleanup();
          if (dragging && destination) { addSection(type, destination.index); announcement.textContent = `${labels[type]} ajouté en position ${destination.index + 1}.`; }
        };
        tile.addEventListener('pointermove', move); tile.addEventListener('pointerup', finish); tile.addEventListener('pointercancel', cancel); document.addEventListener('keydown', escape);
      });
    }
    function beginInline(element) {
      if (!editable || busy || templateMode || inlineEdit) return;
      const holder = element.closest('[data-section-id]'); if (!holder) return;
      const section = doc.sections.find(item => item.id === holder.dataset.sectionId);
      const path = element.dataset.editField;
      if (!section || !/^(title|text|eyebrow|caption|button\.label|items\.[0-3]\.(title|text))$/.test(path || '')) return;
      const parts = path.split('.');
      const read = object => parts.reduce((value, key) => value?.[key], object);
      const original = read(section); if (typeof original !== 'string') return;
      window.clearTimeout(previewTimer); previewVersion++; previewState.hidden = true;
      selected = section.id; library.hidden = true; properties.hidden = false; inspectorTab = 'content'; renderLayers(); renderProperties();
      element.textContent = original; element.contentEditable = 'plaintext-only'; element.classList.add('builder-inline-editing'); element.setAttribute('role', 'textbox'); element.setAttribute('aria-label', `Modifier ${path.endsWith('text') ? 'le texte' : 'le titre'}`); element.tabIndex = 0;
      const document = element.ownerDocument;
      const finish = cancelled => {
        if (!inlineEdit) return;
        const value = element.innerText.replace(/\r\n/g, '\n');
        element.removeEventListener('blur', blur); element.removeEventListener('input', inputChanged); element.removeEventListener('keydown', keydown); element.removeEventListener('paste', paste); element.removeEventListener('drop', preventDrop);
        element.removeAttribute('contenteditable'); element.removeAttribute('role'); element.removeAttribute('aria-label'); element.setAttribute('tabindex', '-1'); element.classList.remove('builder-inline-editing'); inlineEdit = null;
        const max = path === 'button.label' ? 80 : path === 'eyebrow' ? 100 : path === 'caption' ? 500 : path.endsWith('title') ? 200 : section.type === 'text' ? 100000 : 5000;
        if (cancelled) element.textContent = original;
        else if ([...value].length > max) { element.textContent = original; fail(new Error(`Ce texte est limité à ${max} caractères. Modifiez-le dans le panneau Contenu.`)); }
        else if (value !== original) change(document => { let node = document.sections.find(item => item.id === section.id); for (const key of parts.slice(0, -1)) node = node[key]; node[parts.at(-1)] = value; }, { properties: true });
        updateState(); queuePreview(0); positionContextToolbar();
      };
      const inputChanged = () => { config.onDirty(dirty() || element.innerText !== original); saveState.textContent = 'Modification du texte en cours…'; saveState.classList.add('is-dirty'); };
      const blur = () => finish(false);
      const keydown = event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish(true); } else if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); event.stopPropagation(); finish(false); } };
      const paste = event => {
        event.preventDefault(); const text = event.clipboardData?.getData('text/plain') || '';
        const selection = document.getSelection(); if (!selection?.rangeCount) return; const range = selection.getRangeAt(0); if (!element.contains(range.commonAncestorContainer)) return;
        range.deleteContents(); const node = document.createTextNode(text); range.insertNode(node); range.setStartAfter(node); range.collapse(true); selection.removeAllRanges(); selection.addRange(range); inputChanged();
      };
      const preventDrop = event => event.preventDefault();
      element.addEventListener('blur', blur); element.addEventListener('input', inputChanged); element.addEventListener('keydown', keydown); element.addEventListener('paste', paste); element.addEventListener('drop', preventDrop);
      inlineEdit = { element, finish }; element.focus(); const range = document.createRange(); range.selectNodeContents(element); const selection = document.getSelection(); selection.removeAllRanges(); selection.addRange(range); positionContextToolbar();
    }
    function attachDrag(handle, row, id) {
      handle.addEventListener('pointerdown', event => {
        if (!editable || busy || event.button !== 0) return;
        event.preventDefault(); handle.setPointerCapture(event.pointerId); row.classList.add('is-dragging');
        let targetIndex = doc.sections.findIndex(item => item.id === id);
        const move = moving => {
          const rows = [...layers.querySelectorAll('[data-section-layer]')];
          targetIndex = rows.findIndex(item => moving.clientY < item.getBoundingClientRect().top + item.getBoundingClientRect().height / 2);
          if (targetIndex === -1) targetIndex = rows.length;
          for (const item of rows) item.classList.remove('drop-before', 'drop-after');
          if (targetIndex < rows.length) rows[targetIndex].classList.add('drop-before'); else rows.at(-1)?.classList.add('drop-after');
          const bounds = layers.getBoundingClientRect();
          if (moving.clientY < bounds.top + 60) layers.scrollTop -= 15;
          if (moving.clientY > bounds.bottom - 60) layers.scrollTop += 15;
        };
        const finish = released => {
          handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', finish); handle.removeEventListener('pointercancel', cancel);
          if (handle.hasPointerCapture(released.pointerId)) handle.releasePointerCapture(released.pointerId);
          const origin = doc.sections.findIndex(item => item.id === id);
          const destination = targetIndex > origin ? targetIndex - 1 : targetIndex;
          if (destination !== origin) change(value => { const [section] = value.sections.splice(origin, 1); value.sections.splice(destination, 0, section); });
          renderLayers(); announcement.textContent = `Section déplacée en position ${destination + 1}.`;
        };
        const cancel = cancelled => { handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', finish); handle.removeEventListener('pointercancel', cancel); if (handle.hasPointerCapture(cancelled.pointerId)) handle.releasePointerCapture(cancelled.pointerId); renderLayers(); };
        handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', finish); handle.addEventListener('pointercancel', cancel);
      });
    }
    function renderLayers() {
      layers.replaceChildren();
      if (templateMode) {
        for (const [key, text] of [['header', 'En-tête'], ['footer', 'Pied de page']]) {
          const item = button(text, () => { select(key); showPanel('properties'); }, 'layout', `builder-template-layer${selected === key ? ' is-selected' : ''}`);
          item.setAttribute('aria-pressed', String(selected === key)); layers.append(item);
        }
        return;
      }
      if (!doc.sections.length) layers.append(h('p', { class: 'builder-empty-sections' }, 'Votre page est prête à recevoir sa première section.'));
      doc.sections.forEach((section, index) => {
        const row = h('div', { class: `builder-layer${selected === section.id ? ' is-selected' : ''}`, 'data-section-layer': section.id });
        const handle = button('', null, 'grip', 'builder-drag-handle'); handle.setAttribute('aria-label', `Déplacer ${labels[section.type]}`); handle.title = 'Glisser pour réordonner';
        const choose = button('', () => { select(section.id); showPanel('properties'); }, null, 'builder-layer-select'); choose.setAttribute('aria-pressed', String(selected === section.id));
        choose.append(h('span', { class: 'builder-layer-index' }, String(index + 1).padStart(2, '0')), h('span', {}, h('strong', {}, labels[section.type]), h('small', {}, section.title || section.caption || section.alt || 'À personnaliser')));
        row.append(editable ? handle : null, choose);
        if (editable && selected === section.id) {
          const up = button('Monter', () => reorder(section.id, -1), 'up', 'icon-only'); up.setAttribute('aria-label', 'Monter la section'); up.disabled = index === 0; up.dataset.unavailable = String(index === 0);
          const down = button('Descendre', () => reorder(section.id, 1), 'down', 'icon-only'); down.setAttribute('aria-label', 'Descendre la section'); down.disabled = index === doc.sections.length - 1; down.dataset.unavailable = String(index === doc.sections.length - 1);
          row.append(h('div', { class: 'builder-layer-tools' }, up, down));
        }
        layers.append(row); attachDrag(handle, row, section.id);
      });
    }
    function field(label, value, onChange, options = {}) {
      const id = `builder-field-${options.key || uuid()}`;
      const input = h(options.multiline ? 'textarea' : 'input', { id, value: value ?? '', type: options.type || 'text', maxlength: options.maxlength || (options.multiline ? 12000 : 500), rows: options.rows || 4, placeholder: options.placeholder, disabled: !editable || busy });
      input.addEventListener('input', () => onChange(input.value)); input.addEventListener('blur', () => { lastMerge = ''; });
      return h('div', { class: 'builder-field' }, h('label', { for: id }, label), input, options.hint ? h('p', {}, options.hint) : null);
    }
    function selectField(label, value, choices, onChange) {
      const id = `builder-select-${uuid()}`;
      const input = h('select', { id, disabled: !editable || busy }, choices.map(([key, text]) => h('option', { value: key }, text))); input.value = value; input.addEventListener('change', () => onChange(input.value));
      return h('div', { class: 'builder-field' }, h('label', { for: id }, label), input);
    }
    function currentSection() { return templateMode ? null : doc.sections.find(item => item.id === selected); }
    function styleChange(key, value, responsive = false, refresh = false) {
      const section = currentSection(); if (!section) return;
      change(document => {
        const item = document.sections.find(block => block.id === section.id);
        item.style ||= {};
        const settings = responsive ? (item.style[device] ||= {}) : item.style;
        if (value === undefined || value === '' || value === null) delete settings[key]; else settings[key] = value;
      }, { merge: `${section.id}:style:${responsive ? device : 'all'}:${key}`, properties: refresh });
    }
    function devicePicker() {
      const group = h('div', { class: 'builder-responsive-picker', role: 'group', 'aria-label': 'Appareil pour les réglages' });
      for (const [key, name, symbol] of [['desktop', 'Ordinateur', 'desktop'], ['tablet', 'Tablette', 'tablet'], ['mobile', 'Mobile', 'mobile']]) {
        const item = button(name, () => setDevice(key), symbol, 'icon-only'); item.setAttribute('aria-pressed', String(device === key)); item.setAttribute('aria-label', name); item.dataset.viewControl = 'true'; item.title = name; group.append(item);
      }
      return h('div', { class: 'builder-responsive-header' }, h('span', {}, 'Réglages par appareil'), group);
    }
    function styleSelect(label, key, choices, options = {}) {
      const section = currentSection();
      const settings = options.responsive ? section.style?.[device] || {} : section.style || {};
      const raw = settings[key];
      const fieldNode = selectField(label, raw == null || raw === 'inherit' || raw === 'default' ? '' : String(raw), [['', 'Hériter'], ...choices], value => styleChange(key, value === '' ? undefined : options.number ? Number(value) : value, options.responsive, true));
      const reset = button('Réinitialiser', () => styleChange(key, undefined, options.responsive, true), 'reset', 'builder-field-reset');
      reset.setAttribute('aria-label', `Réinitialiser ${label.toLowerCase()}`); reset.disabled = raw == null; reset.dataset.unavailable = String(raw == null);
      fieldNode.querySelector('label').append(reset);
      return fieldNode;
    }
    function palette(label, key, values) {
      const value = currentSection().style?.[key] || 'default';
      const grid = h('div', { class: 'builder-color-grid', role: 'group', 'aria-label': label, 'data-palette': key });
      for (const [color, name] of values) {
        const choice = button('', () => styleChange(key, color === 'default' ? undefined : color, false, true), null, 'builder-color-choice');
        choice.dataset.color = color; choice.title = name; choice.setAttribute('aria-label', `${label} : ${name}`); choice.setAttribute('aria-pressed', String(value === color)); choice.append(color === value ? icon('check') : h('span', { 'aria-hidden': 'true' })); grid.append(choice);
      }
      return h('div', { class: 'builder-field builder-color-field' }, h('span', { class: 'builder-field-label' }, label, h('span', {}, values.find(([key]) => key === value)?.[1] || 'Hériter')), grid);
    }
    function renderStyleProperties(section) {
      const colors = [['default', 'Hériter'], ['ink', 'Encre'], ['muted', 'Gris'], ['white', 'Blanc'], ['forest', 'Forêt'], ['purple', 'Violet'], ['blue', 'Bleu']];
      const backgrounds = [['default', 'Hériter'], ['white', 'Blanc'], ['cream', 'Crème'], ['sage', 'Sauge'], ['forest', 'Forêt'], ['ink', 'Encre'], ['purple', 'Violet'], ['blue', 'Bleu'], ['rose', 'Rose']];
      const sizes = [12,14,16,18,20,24,28,32,40,48,56,64,80,96].map(value => [String(value), `${value} px`]);
      propsBody.append(h('section', { class: 'builder-property-group' }, h('h3', {}, 'Couleurs'), palette('Arrière-plan', 'background', backgrounds), palette('Texte', 'text_color', colors)),
        h('section', { class: 'builder-property-group' }, h('h3', {}, 'Typographie'), styleSelect('Police', 'font_family', [['sans', 'Sans empattement'], ['serif', 'Avec empattement'], ['mono', 'Monospace']]), styleSelect('Graisse', 'font_weight', [['normal', 'Normale'], ['medium', 'Moyenne'], ['semibold', 'Semi-grasse'], ['bold', 'Grasse']]), devicePicker(), styleSelect('Taille des titres', 'title_size', sizes, { responsive: true, number: true }), styleSelect('Taille du texte', 'text_size', sizes, { responsive: true, number: true }), styleSelect('Alignement', 'align', [['left', 'À gauche'], ['center', 'Centré'], ['right', 'À droite']], { responsive: true })),
        h('section', { class: 'builder-property-group' }, h('h3', {}, 'Bordure'), styleSelect('Épaisseur', 'border_width', [0,1,2,4].map(value => [String(value), `${value} px`]), { number: true }), palette('Couleur de bordure', 'border_color', colors), styleSelect('Arrondi', 'border_radius', [0,4,8,12,16,24,32,48].map(value => [String(value), `${value} px`]), { number: true })));
    }
    function edgeControls(label, key) {
      const values = currentSection().style?.[device]?.[key];
      const group = h('div', { class: 'builder-edge-group' });
      const reset = button('Réinitialiser', () => styleChange(key, undefined, true, true), 'reset', 'builder-field-reset'); reset.setAttribute('aria-label', `Réinitialiser ${label.toLowerCase()}`); reset.disabled = !values; reset.dataset.unavailable = String(!values);
      group.append(h('div', { class: 'builder-edge-heading' }, h('h4', {}, label), h('span', {}, values ? 'px' : 'Hérité'), reset));
      const sides = h('div', { class: 'builder-edges' });
      for (const [side, name] of [['top', 'Haut'], ['right', 'Droite'], ['bottom', 'Bas'], ['left', 'Gauche']]) {
        sides.append(selectField(name, values ? String(values[side]) : '', [['', '—'], ...[0,4,8,12,16,24,32,48,64,80,96,120].map(value => [String(value), String(value)])], value => styleChange(key, value === '' ? undefined : { top: 0, right: 0, bottom: 0, left: 0, ...values, [side]: Number(value) }, true, true)));
      }
      group.append(sides); return group;
    }
    function renderAdvancedProperties(section) {
      propsBody.append(h('section', { class: 'builder-property-group' }, h('h3', {}, 'Mise en page'), devicePicker(), styleSelect('Largeur du contenu', 'content_width', [0,480,640,800,960,1120,1280].map(value => [String(value), value ? `${value} px` : 'Pleine largeur']), { responsive: true, number: true }), edgeControls('Marge intérieure', 'padding'), edgeControls('Marge extérieure', 'margin'), h('p', { class: 'builder-control-help' }, 'Les valeurs définies pour un appareil remplacent celles héritées. Réinitialisez un réglage pour retrouver le comportement du modèle.')));
      const visibility = h('section', { class: 'builder-property-group' }, h('h3', {}, 'Visibilité responsive'));
      for (const [key, name, symbol] of [['desktop', 'ordinateur', 'desktop'], ['tablet', 'tablette', 'tablet'], ['mobile', 'mobile', 'mobile']]) {
        const id = `visibility-${key}`;
        const input = h('input', { type: 'checkbox', id, checked: Boolean(section.style?.[`hidden_${key}`]), onChange: event => styleChange(`hidden_${key}`, event.target.checked ? true : undefined) });
        visibility.append(h('label', { class: 'builder-visibility-toggle', for: id }, icon(symbol), h('span', {}, `Masquer sur ${name}`), input));
      }
      visibility.append(h('p', { class: 'builder-control-help' }, 'Un élément masqué reste disponible dans Structure pour être modifié ou réaffiché.'));
      propsBody.append(visibility);
    }
    function renderProperties() {
      propsBody.replaceChildren();
      for (const tab of inspectorTabs.children) tab.setAttribute('aria-selected', String(tab.dataset.inspectorTab === inspectorTab));
      if (templateMode) { renderTemplateProperties(); return; }
      const section = doc.sections.find(item => item.id === selected);
      if (!section) { propsBody.append(h('div', { class: 'builder-property-empty' }, icon('layout'), h('h3', {}, 'Votre page prend forme ici.'), h('p', {}, 'Ajoutez une section, puis personnalisez son contenu et son apparence.'))); return; }
      inspectorTitle.textContent = `Modifier ${labels[section.type].toLowerCase()}`;
      if (inspectorTab === 'style') { renderStyleProperties(section); updateState(); return; }
      if (inspectorTab === 'advanced') { renderAdvancedProperties(section); updateState(); return; }
      const update = (key, value, refresh = false) => change(document => { document.sections.find(item => item.id === section.id)[key] = value; }, { merge: `${section.id}:${key}`, properties: refresh });
      const group = h('section', { class: 'builder-property-group' }, h('h3', {}, 'Contenu'));
      if (section.type === 'divider') group.append(h('p', { class: 'builder-control-help' }, 'Une ligne pour séparer deux parties de votre page. Personnalisez son fond et son espacement dans Style et Avancé.'));
      if (section.type === 'spacer') group.append(selectField('Hauteur de l’espace', String(section.height), [8,16,24,32,48,64,96,128,192,240].map(value => [String(value), `${value} px`]), value => update('height', Number(value))));
      if (section.type === 'hero') group.append(field('Petit titre', section.eyebrow, value => update('eyebrow', value), { maxlength: 100 }));
      if ('title' in section) group.append(field('Titre', section.title, value => update('title', value), { maxlength: 200, multiline: true, rows: 2 }));
      if ('text' in section) group.append(field('Texte', section.text, value => update('text', value), { multiline: true, maxlength: section.type === 'text' ? 100000 : 5000, hint: 'Séparez vos paragraphes par une ligne vide.' }));
      if (section.type === 'hero' || section.type === 'image') {
        group.append(field('Adresse de l’image', section.type === 'hero' ? section.image_url : section.url, value => update(section.type === 'hero' ? 'image_url' : 'url', section.type === 'hero' ? value || null : value), { maxlength: 2048, placeholder: 'https://…', hint: 'Utilisez une adresse HTTPS ou une image déjà disponible dans /assets/.', multiline: true, rows: 2 }));
        group.append(field('Description de l’image', section.type === 'hero' ? section.image_alt : section.alt, value => update(section.type === 'hero' ? 'image_alt' : 'alt', value), { maxlength: 300, hint: 'Décrivez ce que montre l’image pour les visiteurs qui ne la voient pas.' }));
      }
      if (section.type === 'image') group.append(field('Légende', section.caption, value => update('caption', value), { maxlength: 500 }));
      if (section.type === 'hero' || section.type === 'cta' || section.type === 'button') {
        if (section.type === 'hero') group.append(selectField('Bouton', section.button ? 'on' : 'off', [['on', 'Afficher un bouton'], ['off', 'Sans bouton']], value => update('button', value === 'on' ? { label: 'En savoir plus', href: '/a-propos' } : null, true)));
        if (section.button) {
          const updateButton = (key, value) => change(document => { document.sections.find(item => item.id === section.id).button[key] = value; }, { merge: `${section.id}:button:${key}` });
          group.append(field('Texte du bouton', section.button.label, value => updateButton('label', value), { maxlength: 80 }), field('Destination du bouton', section.button.href, value => updateButton('href', value), { maxlength: 2048, placeholder: '/a-propos', hint: 'Une adresse du site, comme /a-propos, ou un lien HTTPS.' }));
        }
      }
      if (section.type === 'columns') {
        section.items.forEach((item, index) => {
          const setItem = (key, value) => change(document => { document.sections.find(block => block.id === section.id).items[index][key] = value; }, { merge: `${section.id}:column:${index}:${key}` });
          const column = h('div', { class: 'builder-column-properties' }, h('h4', {}, `Colonne ${index + 1}`), field('Titre', item.title, value => setItem('title', value), { maxlength: 200 }), field('Texte', item.text, value => setItem('text', value), { multiline: true, maxlength: 5000 }));
          if (editable && section.items.length > 2) column.append(button('Supprimer cette colonne', () => change(document => { document.sections.find(block => block.id === section.id).items.splice(index, 1); }, { properties: true }), 'trash', 'text-danger'));
          group.append(column);
        });
        if (editable && section.items.length < 4) group.append(button('Ajouter une colonne', () => change(document => { document.sections.find(block => block.id === section.id).items.push({ title: 'Une nouvelle idée', text: 'Présentez cette idée en quelques mots.' }); }, { properties: true }), 'plus', 'secondary'));
      }
      group.append(selectField('Alignement de base', section.align, [['left', 'À gauche'], ['center', 'Centré'], ['right', 'À droite']], value => update('align', value)), selectField('Espacement de base', section.spacing, [['small', 'Compact'], ['medium', 'Équilibré'], ['large', 'Aéré']], value => update('spacing', value)));
      propsBody.append(group);
      if (editable) propsBody.append(h('div', { class: 'builder-section-actions' }, button('Dupliquer', duplicateSelected, 'copy', 'secondary'), button('Supprimer', deleteSelected, 'trash', 'text-danger')));
      updateState();
    }
    function renderTemplateProperties() {
      const area = selected === 'footer' ? 'footer' : 'header';
      const section = doc[area];
      const title = area === 'header' ? 'En-tête' : 'Pied de page';
      inspectorTitle.textContent = title;
      propsBody.append(h('section', { class: 'builder-property-group' }, h('div', { class: 'builder-selection-title' }, h('p', { class: 'builder-eyebrow' }, 'PARTIE COMMUNE'), h('h3', {}, title)),
        selectField('Modèle', section.model, area === 'header' ? [['classic', 'Logo à gauche · menu à droite'], ['centered', 'Logo centré · menu dessous']] : [['simple', 'Liens sur une ligne'], ['columns', 'Présentation en colonnes']], value => change(document => { document[area].model = value; }))));
      if (area === 'footer') propsBody.append(h('section', { class: 'builder-property-group' }, field('Présentation', section.description, value => change(document => { document.footer.description = value; }, { merge: 'footer:description' }), { multiline: true, maxlength: 1000, hint: 'Une courte présentation affichée dans le pied de page.' })));
      const links = h('section', { class: 'builder-property-group builder-menu-properties' }, h('h3', {}, 'Liens du menu'));
      const pages = structure.pages.filter(page => page.kind === 'system' || page.content_id);
      const options = pages.map(page => [page.kind === 'system' ? `builtin:${{ home: 'home', about: 'about', blog: 'blog', account: 'account' }[page.id] || page.id}` : `page:${page.content_id}`, `${page.title}${page.status === 'draft' ? ' · brouillon' : ''}`]);
      const targetValue = value => value.kind === 'builtin' ? `builtin:${value.page}` : `page:${value.id}`;
      section.links.forEach((item, index) => {
        const group = h('div', { class: 'builder-menu-item' }, h('h4', {}, `Lien ${index + 1}`), field('Libellé', item.label, value => change(document => { document[area].links[index].label = value; }, { merge: `${area}:link:${item.id}:label` }), { maxlength: 80 }));
        const values = [...options]; if (!values.some(([value]) => value === targetValue(item.target))) values.push([targetValue(item.target), 'Page indisponible']);
        group.append(selectField('Page de destination', targetValue(item.target), values, value => change(document => { const [kind, id] = value.split(':'); document[area].links[index].target = kind === 'builtin' ? { kind, page: id } : { kind, id }; })));
        if (editable) {
          const moveLink = direction => change(document => { const [link] = document[area].links.splice(index, 1); document[area].links.splice(index + direction, 0, link); }, { properties: true });
          const up = button('Monter', () => moveLink(-1), 'up', 'icon-only'); up.setAttribute('aria-label', `Monter le lien ${index + 1}`); up.disabled = index === 0; up.dataset.unavailable = String(index === 0);
          const down = button('Descendre', () => moveLink(1), 'down', 'icon-only'); down.setAttribute('aria-label', `Descendre le lien ${index + 1}`); down.disabled = index === section.links.length - 1; down.dataset.unavailable = String(index === section.links.length - 1);
          group.append(h('div', { class: 'builder-menu-actions' }, up, down, button('Supprimer', () => change(document => { document[area].links.splice(index, 1); }, { properties: true }), 'trash', 'text-danger')));
        }
        links.append(group);
      });
      if (editable && section.links.length < (area === 'header' ? 10 : 20)) links.append(button('Ajouter un lien', () => change(document => { document[area].links.push({ id: uuid(), label: 'Accueil', target: { kind: 'builtin', page: 'home' } }); }, { properties: true }), 'plus', 'secondary'));
      propsBody.append(links, h('p', { class: 'builder-property-note' }, 'Publier ce modèle met à jour les menus de toutes les pages de ce site. Les autres sites conservent leur propre modèle.'));
      updateState();
    }
    function markSelection() {
      try {
        const document = frame.contentDocument;
        if (!document) return;
        for (const element of document.querySelectorAll('[data-section-id], [data-template-area]')) element.toggleAttribute('data-builder-selected', templateMode ? element.dataset.templateArea === selected : element.dataset.sectionId === selected);
        for (const element of document.querySelectorAll('[data-builder-selected]')) element.setAttribute('data-builder-selected', 'true');
        positionContextToolbar();
      } catch { /* Preview may be loading. */ }
    }
    function resizeFrame() {
      const document = frame.contentDocument;
      if (!document?.body || !stage.clientWidth) return;
      const padding = getComputedStyle(stage);
      const available = stage.clientWidth - parseFloat(padding.paddingLeft) - parseFloat(padding.paddingRight);
      const width = device === 'mobile' ? 390 : device === 'tablet' ? 820 : 1024;
      const scale = Math.min(1, Math.max(0.1, available / width));
      frame.style.width = `${width}px`;
      frame.style.height = '620px';
      frame.style.transform = `scale(${scale})`;
      canvas.style.width = `${width * scale}px`;
      window.requestAnimationFrame(() => {
        try {
          const height = Math.max(620, document.documentElement.scrollHeight, document.body.scrollHeight);
          frame.style.height = `${height}px`; canvas.style.height = `${height * scale}px`; positionContextToolbar();
        } catch { /* Keep the previous preview while the document loads. */ }
      });
    }
    const stageObserver = new ResizeObserver(resizeFrame); stageObserver.observe(stage);
    frame.addEventListener('load', () => {
      try {
        const document = frame.contentDocument;
        if (!document?.body?.hasAttribute('data-template-preview')) { previewState.textContent = 'L’aperçu n’a pas pu être affiché. Réessayez.'; return; }
        document.body.removeAttribute('inert'); document.body.setAttribute('data-builder-preview', 'true');
        for (const element of document.querySelectorAll('[inert]')) element.removeAttribute('inert');
        for (const element of document.querySelectorAll('a,button,input,textarea,select')) element.setAttribute('tabindex', '-1');
        document.addEventListener('click', event => {
          if (inlineEdit?.element.contains(event.target)) { event.stopPropagation(); if (event.target.closest('a')) event.preventDefault(); return; }
          event.preventDefault(); event.stopPropagation();
          const item = event.target.closest(templateMode ? '[data-template-area]' : '[data-section-id]');
          if (item) select(templateMode ? item.dataset.templateArea : item.dataset.sectionId, true);
        }, true);
        document.addEventListener('dblclick', event => { const field = event.target.closest('[data-edit-field]'); if (field && editable && !templateMode) { event.preventDefault(); event.stopPropagation(); beginInline(field); } }, true);
        document.addEventListener('submit', event => event.preventDefault(), true);
        document.addEventListener('auxclick', event => event.preventDefault(), true);
        previewState.hidden = true; canvas.hidden = false; markSelection(); resizeFrame();
        for (const image of document.images) image.addEventListener('load', resizeFrame, { once: true });
      } catch { previewState.textContent = 'L’aperçu n’a pas pu être affiché. Réessayez.'; }
    });
    function queuePreview(delay = 450) { window.clearTimeout(previewTimer); if (!inlineEdit) previewTimer = window.setTimeout(loadPreview, delay); }
    async function loadPreview() {
      if (inlineEdit) return;
      const version = ++previewVersion;
      previewState.hidden = false; previewState.textContent = 'Mise à jour de l’aperçu…';
      try {
        const html = await config.preview(`${base}/preview`, { document: clone(doc) });
        if (version !== previewVersion || inlineEdit) return;
        frame.srcdoc = html;
      } catch (error) {
        if (version !== previewVersion) return;
        previewState.replaceChildren(h('span', {}, error.message || 'L’aperçu n’a pas pu être actualisé.'), button('Réessayer', loadPreview, null, 'secondary'));
      }
    }
    async function persist() {
      if (!dirty() && savedInEditor) return;
      const saved = await config.api(base, { method: 'PUT', body: { expected_revision: meta.revision, document: clone(doc) } });
      meta = saved; savedInEditor = true; doc = clone(saved.document); baseline = JSON.stringify(doc); config.onDirty(false); renderLayers(); renderProperties(); queuePreview(0);
    }
    async function saveDraft() {
      if (busy || !editable) return;
      if (inlineEdit) inlineEdit.finish(false);
      busy = true; alert.hidden = true; updateState();
      try { await persist(); config.toast('Le brouillon est enregistré.'); }
      catch (error) { fail(error); }
      finally { busy = false; updateState(); }
    }
    async function publishDocument() {
      if (busy || !editable) return;
      if (inlineEdit) inlineEdit.finish(false);
      busy = true; alert.hidden = true; updateState();
      try {
        await persist();
        await config.api(publishPath, { method: 'POST', body: { expected_revision: meta.revision } });
        meta = await config.api(base); doc = clone(meta.document); baseline = JSON.stringify(doc); config.onDirty(false); renderLayers(); renderProperties(); queuePreview(0);
        config.toast(templateMode ? 'Le modèle est publié sur toutes les pages de ce site.' : 'La page est publiée sur votre vitrine.');
      } catch (error) { fail(error); }
      finally { busy = false; updateState(); }
    }
    renderLibrary(); renderLayers(); renderProperties(); updateState(); await loadPreview();
  }
  return { mount };
})();
