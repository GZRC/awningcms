(() => {
  'use strict';
  const state = { site: null, visitor: null, mode: 'login', busy: false };
  const app = document.getElementById('account-app');
  const esc = value => String(value ?? '').replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]));
  const userIcon = '<img src="/assets/icons/user-round.svg" width="24" height="24" alt="">';
  let noticeTimer;

  async function api(path, method = 'GET', data) {
    let response;
    try {
      response = await fetch(`/mon-compte/api/${path}`, {
        method, credentials: 'same-origin', cache: 'no-store',
        headers: data === undefined ? {} : { 'Content-Type': 'application/json' },
        body: data === undefined ? undefined : JSON.stringify(data)
      });
    } catch (_) {
      throw new Error('La connexion a été interrompue. Vérifiez votre réseau et réessayez.');
    }
    const result = response.status === 204 ? null : await response.json().catch(() => null);
    if (!response.ok) {
      const error = new Error(result?.error || 'Impossible de terminer cette action. Réessayez.');
      error.status = response.status;
      throw error;
    }
    return result;
  }

  function announce(message) {
    const element = document.getElementById('account-notice');
    clearTimeout(noticeTimer);
    element.textContent = message;
    element.classList.add('is-visible');
    noticeTimer = setTimeout(() => element.classList.remove('is-visible'), 5000);
  }

  function showError(message) {
    const error = document.getElementById('account-error');
    error.textContent = message;
    error.hidden = false;
    error.focus();
  }

  function setBusy(busy) {
    state.busy = busy;
    const form = document.getElementById('account-form');
    if (form) form.setAttribute('aria-busy', String(busy));
    app.querySelectorAll('button').forEach(button => { button.disabled = busy; });
    const submit = document.getElementById('account-submit');
    if (submit) submit.textContent = busy ? 'Un instant…' : state.visitor ? 'Enregistrer les modifications' : state.mode === 'register' ? 'Créer mon compte' : 'Me connecter';
  }

  function shell(content) {
    const name = state.site.name;
    app.innerHTML = `
      <div class="account-layout">
        <section class="account-intro" aria-labelledby="account-title">
          <a class="account-return" href="/"><span aria-hidden="true">←</span> Retour au site</a>
          <span class="account-eyebrow">VOTRE ESPACE PERSONNEL</span>
          <h1 id="account-title">${state.visitor ? 'Heureux de vous <br>retrouver.' : 'Bienvenue <br>chez vous.'}</h1>
          <p class="account-intro-copy">${state.visitor ? 'Retrouvez vos informations personnelles et gardez votre profil à jour.' : `Un espace à vous, sur ${esc(name)}. Connectez-vous ou créez votre compte pour nous rejoindre.`}</p>
          <div class="account-intro-note"><span class="account-note-icon">${userIcon}</span><div><strong>Votre compte sur ${esc(name)}</strong><p>Vos informations, au même endroit.</p></div></div>
        </section>
        <section class="account-card" aria-labelledby="account-card-title">${content}</section>
      </div>`;
    document.title = `Mon compte · ${name}`;
  }

  function authForm(focus = false) {
    const register = state.mode === 'register';
    shell(`
      <div class="account-modes" aria-label="Accès au compte">
        <button type="button" data-mode="login" aria-pressed="${!register}">Connexion</button>
        <button type="button" data-mode="register" aria-pressed="${register}">Créer un compte</button>
      </div>
      <h2 id="account-card-title">${register ? 'Faisons connaissance.' : 'Ravi de vous revoir.'}</h2>
      <p class="account-card-copy">${register ? 'Quelques informations pour créer votre espace.' : 'Entrez vos identifiants pour accéder à votre compte.'}</p>
      <div id="account-error" class="account-error" role="alert" tabindex="-1" hidden></div>
      <form id="account-form">
        ${register ? '<div class="account-field"><label for="account-name">Votre nom</label><input id="account-name" name="name" autocomplete="name" required maxlength="100" placeholder="Prénom et nom"></div>' : ''}
        <div class="account-field"><label for="account-email">Adresse e-mail</label><input id="account-email" name="email" type="email" autocomplete="username" inputmode="email" required maxlength="254" placeholder="vous@exemple.fr" autocapitalize="none" spellcheck="false"></div>
        <div class="account-field"><label for="account-password">Mot de passe</label><div class="account-password-wrap"><input id="account-password" name="password" type="password" autocomplete="${register ? 'new-password' : 'current-password'}" required ${register ? 'minlength="12"' : ''} maxlength="256" ${register ? 'aria-describedby="password-help"' : ''}><button class="account-password-toggle" type="button" aria-controls="account-password" aria-pressed="false">Afficher</button></div>${register ? '<p class="account-field-help" id="password-help">12 caractères minimum. Une phrase facile à retenir fonctionne très bien.</p>' : ''}</div>
        <button class="account-primary" type="submit" id="account-submit">${register ? 'Créer mon compte' : 'Me connecter'}<span aria-hidden="true">→</span></button>
        ${register ? '<p class="account-privacy">Vos informations sont utilisées pour gérer votre compte sur ce site.</p>' : ''}
      </form>`);
    app.querySelectorAll('[data-mode]').forEach(button => button.addEventListener('click', () => {
      if (button.dataset.mode === state.mode) return;
      state.mode = button.dataset.mode;
      authForm(true);
    }));
    app.querySelector('.account-password-toggle').addEventListener('click', event => {
      const input = document.getElementById('account-password');
      const visible = input.type === 'password';
      input.type = visible ? 'text' : 'password';
      event.currentTarget.textContent = visible ? 'Masquer' : 'Afficher';
      event.currentTarget.setAttribute('aria-pressed', String(visible));
    });
    document.getElementById('account-form').addEventListener('submit', async event => {
      event.preventDefault();
      if (state.busy) return;
      document.getElementById('account-error').hidden = true;
      const form = new FormData(event.currentTarget);
      const data = { email: form.get('email'), password: form.get('password') };
      if (register) data.name = form.get('name');
      setBusy(true);
      try {
        state.visitor = await api(register ? 'register' : 'login', 'POST', data);
        profileForm(true);
        announce(register ? 'Votre compte a été créé. Bienvenue !' : 'Vous êtes connecté.');
      } catch (error) {
        showError(error.message);
      } finally { setBusy(false); }
    });
    if (focus) document.getElementById(register ? 'account-name' : 'account-email').focus();
  }

  function profileForm(focus = false) {
    const visitor = state.visitor;
    shell(`
      <div class="account-profile-top"><span class="account-avatar" aria-hidden="true">${esc(Array.from(visitor.name)[0]?.toLocaleUpperCase('fr') || 'V')}</span><span class="account-connected"><span aria-hidden="true"></span> Connecté</span></div>
      <h2 id="account-card-title" tabindex="-1">Bonjour ${esc(visitor.name)}.</h2>
      <p class="account-card-copy">Bienvenue dans votre espace personnel.</p>
      <div id="account-error" class="account-error" role="alert" tabindex="-1" hidden></div>
      <form id="account-form">
        <div class="account-field"><label for="account-name">Votre nom</label><input id="account-name" name="name" autocomplete="name" required maxlength="100" value="${esc(visitor.name)}"></div>
        <div class="account-field"><span class="account-label">Adresse e-mail</span><p class="account-email-value">${esc(visitor.email)}</p></div>
        <button class="account-primary" type="submit" id="account-submit">Enregistrer les modifications</button>
      </form>
      <div class="account-profile-bottom"><a href="/">Continuer ma visite <span aria-hidden="true">↗</span></a><button type="button" id="account-logout">Me déconnecter</button></div>`);
    document.getElementById('account-form').addEventListener('submit', async event => {
      event.preventDefault();
      if (state.busy) return;
      document.getElementById('account-error').hidden = true;
      const data = new FormData(event.currentTarget);
      setBusy(true);
      try {
        state.visitor = await api('profile', 'PUT', { name: data.get('name') });
        profileForm(true);
        announce('Vos informations ont été enregistrées.');
      } catch (error) {
        if (error.status === 401) {
          state.visitor = null;
          authForm(true);
          showError('Votre session a expiré. Connectez-vous de nouveau.');
        } else showError(error.message);
      } finally { setBusy(false); }
    });
    document.getElementById('account-logout').addEventListener('click', async () => {
      if (state.busy) return;
      setBusy(true);
      try {
        await api('logout', 'POST', {});
        state.visitor = null;
        state.mode = 'login';
        authForm(true);
        announce('Vous êtes déconnecté. À bientôt !');
      } catch (error) { showError(error.message); }
      finally { setBusy(false); }
    });
    if (focus) document.getElementById('account-card-title').focus();
  }

  async function start() {
    try {
      const bootstrap = await api('bootstrap');
      state.site = bootstrap.site;
      try { state.visitor = await api('me'); }
      catch (error) { if (error.status !== 401) throw error; }
      if (state.visitor) profileForm(); else authForm();
    } catch (error) {
      app.innerHTML = `<section class="account-loading"><h1>Votre espace est indisponible.</h1><p>${esc(error.message)}</p><button class="account-primary" id="account-retry" type="button">Réessayer</button><a href="/">Retour au site</a></section>`;
      document.getElementById('account-retry').addEventListener('click', start);
    }
  }
  start();
})();
