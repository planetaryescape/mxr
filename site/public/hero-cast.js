(function () {
  var cast = document.querySelector('.hero-cast');
  if (!cast) return;

  // Radios and videos are authored in the same order: cli, tui, agent.
  var radios = Array.from(cast.querySelectorAll('.hero-cast-radio'));
  var videos = Array.from(cast.querySelectorAll('.hero-cast-video'));
  var toggle = cast.querySelector('.hero-cast-motion');
  if (!toggle || radios.length !== videos.length) return;

  var reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  var paused = reducedMotion.matches;

  function selectedVideo() {
    var index = radios.findIndex(function (radio) {
      return radio.checked;
    });
    return videos[index];
  }

  // `ended` and `error` count as stopped too: a recording that ran out or failed
  // to load is not running even where the element still reports paused as false.
  function running(video) {
    return !!video && !video.paused && !video.ended && !video.error;
  }

  // The button reports what the selected video is doing, not what we asked for.
  function label() {
    var next = running(selectedVideo()) ? 'pause' : 'play';
    toggle.textContent = next;
    toggle.setAttribute('aria-label', next + ' demo recording');
  }

  // A switch away and back leaves the video selected again, so the element alone
  // cannot separate a live rejection from one left over by a start we have since
  // replaced. Each start takes the next token, and only the newest one counts.
  var attempt = 0;

  // No pause event follows a start that never happened, so record the stop here.
  function failed(video, token) {
    if (token !== attempt || video !== selectedVideo()) return;
    paused = true;
    label();
  }

  // src is assigned on first play, so unselected recordings, and all of them
  // while motion is paused, never hit the network.
  function play(video) {
    if (!video.getAttribute('src')) video.src = video.dataset.src;
    var token = ++attempt;
    try {
      var started = video.play();
      // Browsers older than the promise-returning play() return undefined.
      if (started) started.catch(function () { failed(video, token); });
    } catch (error) {
      failed(video, token);
    }
  }

  // The poster is all a pane shows before it plays, so the two that start off
  // screen keep theirs, and the src of their fallback image, in data attributes
  // until the first time they are picked.
  function reveal(video) {
    if (!video || !video.dataset.poster) return;
    video.poster = video.dataset.poster;
    delete video.dataset.poster;
    var fallback = video.querySelector('img[data-src]');
    if (fallback) fallback.src = fallback.dataset.src;
  }

  function sync() {
    var active = selectedVideo();
    reveal(active);
    videos.forEach(function (video) {
      if (video === active && !paused) play(video);
      else video.pause();
    });
    label();
  }

  function setPaused(next) {
    paused = next;
    sync();
  }

  radios.forEach(function (radio) {
    radio.addEventListener('change', sync);
  });

  // A stop we did not ask for, from a suspended tab or a recording running out,
  // would leave the next click doing the opposite of the label. Deselected
  // videos say nothing about the selected one. Media events are queued, so a
  // stale one can land on a video that is selected and playing again after a
  // fast switch away and back: read the element, not the event type.
  function track(event) {
    var video = selectedVideo();
    if (event.target !== video) return;
    paused = !running(video);
    label();
  }

  videos.forEach(function (video) {
    ['play', 'pause', 'ended', 'error'].forEach(function (type) {
      video.addEventListener(type, track);
    });
  });

  toggle.addEventListener('click', function () {
    setPaused(!paused);
  });

  reducedMotion.addEventListener('change', function (event) {
    setPaused(event.matches);
  });

  // Labels the control and starts the selected recording unless motion is
  // reduced.
  toggle.hidden = false;
  setPaused(paused);
})();
