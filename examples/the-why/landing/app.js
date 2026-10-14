const root = document.documentElement;
const motion = true;
root.classList.add('motion-on', 'js-motion');
const observer = new IntersectionObserver(entries => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    entry.target.classList.add('visible');
    const counter = entry.target.querySelector('[data-count]');
    if (counter && motion) {
      const target = Number(counter.dataset.count), start = performance.now();
      function tick(now) {
        const p = Math.min((now - start) / 1400, 1);
        counter.textContent = Math.round(target * (1 - (1-p)**3)).toLocaleString('en-US');
        if (p < 1 && motion) requestAnimationFrame(tick);
        else counter.textContent = target.toLocaleString('en-US');
      }
      requestAnimationFrame(tick);
    }
    observer.unobserve(entry.target);
  }
}, { threshold: 0.12 });
document.querySelectorAll('.reveal').forEach(el => observer.observe(el));
const playlist = 'assets/hls/master.m3u8';
// Both players use the same lazy HLS setup and release it when collapsed.
function createPlayback(video, report) {
  let hls = null, initialized = false, pendingTime = null, request = 0;
  video.addEventListener('loadedmetadata', () => {
    if (pendingTime !== null) { video.currentTime = pendingTime; pendingTime = null; }
  });
  function initialize() {
    if (initialized) return true;
    video.querySelector('source')?.remove();
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = playlist;
    } else if (window.Hls?.isSupported()) {
      hls = new Hls({ autoStartLoad: false, capLevelToPlayerSize: true, maxBufferLength: 20 });
      hls.loadSource(playlist);
      hls.attachMedia(video);
      let recoveries = 0;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        if (data.type === Hls.ErrorTypes.MEDIA_ERROR && recoveries++ < 1) hls.recoverMediaError();
        else report('Playback interrupted. Close this preview and try again.');
      });
    } else {
      report('Streaming is not supported in this browser. Try Safari, Chrome, Firefox or Edge.');
      return false;
    }
    initialized = true;
    return true;
  }
  video.addEventListener('error', () => {
    if (initialized && !hls) report('The film could not load. Close this preview and try again.');
  });
  return {
    play(time) {
      const current = ++request;
      report('');
      if (!initialize()) return;
      pendingTime = time;
      if (video.readyState >= 1) { video.currentTime = time; pendingTime = null; }
      hls?.startLoad(time);
      video.play().catch(error => {
        if (current === request && error.name !== 'AbortError') report('Press play in the video controls to start.');
      });
    },
    reset() {
      request++;
      initialized = false;
      pendingTime = null;
      video.pause();
      hls?.destroy();
      hls = null;
      video.removeAttribute('src');
      video.querySelector('source')?.remove();
      video.load();
      report('');
    }
  };
}
const movie = document.querySelector('#movie');
const player = document.querySelector('.player-wrap');
const status = document.querySelector('#player-status');
const closeFilm = document.querySelector('#close-film');
const filmPlayback = createPlayback(movie, message => { status.textContent = message; });
let activeScene = null;
function collapseFilm(restoreFocus = true) {
  filmPlayback.reset();
  player.classList.remove('started');
  closeFilm.hidden = true;
  if (restoreFocus) player.querySelector('[data-play]').focus({ preventScroll: true });
}
closeFilm.addEventListener('click', () => collapseFilm());
movie.addEventListener('play', () => {
  closeScene(false);
  player.classList.add('started');
  closeFilm.hidden = false;
});
player.addEventListener('keydown', event => {
  if (event.key === 'Escape' && player.classList.contains('started')) collapseFilm();
});
document.querySelectorAll('[data-play]').forEach(button => button.addEventListener('click', () => {
  closeScene(false);
  player.classList.add('started');
  closeFilm.hidden = false;
  // Only the hero action navigates. The player itself stays in place.
  if (!player.contains(button)) player.scrollIntoView({ behavior: motion ? 'smooth' : 'instant', block: 'center' });
  filmPlayback.play(Number(button.dataset.play));
}));
function closeScene(restoreFocus = true) {
  if (!activeScene) return;
  const { playback, shell, picture, trigger } = activeScene;
  playback.reset();
  shell.remove();
  picture.classList.remove('is-playing');
  trigger.setAttribute('aria-expanded', 'false');
  activeScene = null;
  if (restoreFocus) trigger.focus({ preventScroll: true });
}
document.querySelectorAll('[data-scene]').forEach(trigger => trigger.addEventListener('click', () => {
  closeScene(false);
  movie.pause();
  const picture = trigger.closest('.scene-picture');
  const shell = document.createElement('div');
  shell.className = 'scene-inline';
  const video = document.createElement('video');
  video.controls = true;
  video.playsInline = true;
  video.preload = 'none';
  video.setAttribute('aria-label', trigger.getAttribute('aria-label').replace('Play scene: ', ''));
  const captions = document.createElement('track');
  Object.assign(captions, { kind: 'captions', src: 'assets/captions.vtt', srclang: 'en', label: 'English' });
  video.append(captions);
  const close = document.createElement('button');
  close.className = 'player-close';
  close.setAttribute('aria-label', 'Close scene preview');
  close.append(document.querySelector('#close-icon').content.cloneNode(true));
  close.addEventListener('click', () => closeScene());
  const message = document.createElement('p');
  message.className = 'scene-error';
  message.setAttribute('role', 'status');
  const playback = createPlayback(video, text => { message.textContent = text; });
  shell.append(video, close, message);
  picture.append(shell);
  picture.classList.add('is-playing');
  trigger.setAttribute('aria-expanded', 'true');
  activeScene = { playback, shell, picture, trigger };
  shell.addEventListener('keydown', event => { if (event.key === 'Escape') closeScene(); });
  const start = Number(trigger.dataset.scene);
  playback.play(start);
  close.focus({ preventScroll: true });
}));
const facts = [...document.querySelectorAll('[data-preview]')];
function selectFact(fact) {
  for (const item of facts) {
    const selected = item === fact;
    item.classList.toggle('is-active', selected);
    item.querySelector('.fact-select').setAttribute('aria-pressed', String(selected));
    const panel = document.querySelector(`#preview-${item.dataset.preview}`);
    panel.classList.toggle('is-active', selected);
    panel.setAttribute('aria-hidden', String(!selected));
    panel.inert = !selected;
  }
}
for (const fact of facts) {
  fact.addEventListener('pointerenter', event => { if (event.pointerType !== 'touch') selectFact(fact); });
  fact.addEventListener('focusin', () => selectFact(fact));
  fact.addEventListener('click', () => selectFact(fact));
}
const progress = document.querySelector('.reading-progress');
const heroImage = document.querySelector('.hero-image');
let scrollQueued = false;
function updateScroll() {
  progress.style.transform = `scaleX(${scrollY / Math.max(1, document.documentElement.scrollHeight-innerHeight)})`;
  if (motion && scrollY < innerHeight) heroImage.style.translate = `0 ${scrollY * .15}px`;
  else heroImage.style.translate = '0 0';
  scrollQueued = false;
}
addEventListener('scroll', () => { if (!scrollQueued) { scrollQueued = true; requestAnimationFrame(updateScroll); } }, { passive: true });
updateScroll();
const canvas = document.querySelector('#dust');
const ctx = canvas.getContext('2d');
let w, h;
function resize() {
  w = canvas.clientWidth; h = canvas.clientHeight;
  const dpr = Math.min(devicePixelRatio || 1, 2);
  canvas.width = w * dpr; canvas.height = h * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}
resize();
addEventListener('resize', resize);
const particles = Array.from({length: 38}, () => ({ x: Math.random(), y: Math.random(), r: Math.random()*1.3+.3, speed: Math.random()*.000018+.000007, phase: Math.random()*6 }));
let last = 0;
function draw(now) {
  const delta = Math.min(now-last, 40); last = now;
  if (motion && !document.hidden && scrollY < h + 100) {
    ctx.clearRect(0, 0, w, h);
    for (const p of particles) {
      p.y -= p.speed * delta;
      if (p.y < 0) p.y = 1;
      ctx.beginPath();
      ctx.fillStyle = `rgba(255,215,155,${.15 + (Math.sin(now*.0007+p.phase)+1)*.2})`;
      ctx.arc(p.x*w+Math.sin(now*.00015+p.phase)*18, p.y*h, p.r, 0, Math.PI*2);
      ctx.fill();
    }
  }
  requestAnimationFrame(draw);
}
requestAnimationFrame(draw);

// Slide the original landing-page wordmark into place when it enters view.
const footerMark = document.querySelector('.footer-mark');
if (footerMark) {
  const footerObserver = new IntersectionObserver(entries => {
    if (entries.some(entry => entry.isIntersecting)) {
      footerMark.classList.add('is-visible');
      footerObserver.disconnect();
    }
  }, { threshold: 0.3 });
  footerObserver.observe(footerMark);
}
