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

  // No pause event follows a start that never happened, so record the stop here.
  // A pane switch while play() was pending makes the rejection stale.
  function failed(video) {
    if (video !== selectedVideo()) return;
    paused = true;
    label();
  }

  // src is assigned on first play, so unselected recordings, and all of them
  // while motion is paused, never hit the network.
  function play(video) {
    if (!video.getAttribute('src')) video.src = video.dataset.src;
    try {
      var started = video.play();
      // Browsers older than the promise-returning play() return undefined.
      if (started) started.catch(function () { failed(video); });
    } catch (error) {
      failed(video);
    }
  }

  function sync() {
    var active = selectedVideo();
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
