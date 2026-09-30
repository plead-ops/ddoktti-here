/* HTML-only component; callers provide data and actions. No Tauri dependency. */
window.createSpeechBubble = function createSpeechBubble(host, onAction) {
  const bubble = document.createElement('section');
  bubble.className = 'speech-bubble';
  bubble.setAttribute('aria-label', '똑띠 알림');
  bubble.hidden = true;
  const close = document.createElement('button');
  close.type = 'button'; close.className = 'bubble-close'; close.textContent = '×';
  close.setAttribute('aria-label', '알림 닫기');
  close.addEventListener('click', () => onAction('dismiss'));
  const meta = document.createElement('div'); meta.className = 'bubble-meta';
  const dot = document.createElement('span'); dot.className = 'bubble-meta-dot';
  const label = document.createElement('span'); meta.append(dot,label);
  const title = document.createElement('h2'); title.className = 'bubble-title';
  const body = document.createElement('p'); body.className = 'bubble-body';
  const detail = document.createElement('p'); detail.className = 'bubble-detail';
  const actions = document.createElement('div'); actions.className = 'bubble-actions';
  bubble.append(close,meta,title,body,detail,actions); host.append(bubble);
  return {
    show(data, privateMode = false) {
      bubble.dataset.kind = data.kind;
      label.textContent = data.label;
      title.textContent = privateMode && data.privateTitle ? data.privateTitle : data.title;
      body.textContent = privateMode ? data.privateBody : data.body;
      detail.textContent = privateMode ? '' : data.detail;
      detail.hidden = !detail.textContent;
      actions.replaceChildren();
      for (const [index,action] of data.actions.entries()) {
        const button = document.createElement('button'); button.type = 'button';
        button.className = 'bubble-action' + (index === 0 ? ' primary' : '');
        button.textContent = action.label;
        button.addEventListener('click', () => onAction(action.id)); actions.append(button);
      }
      bubble.hidden = false;
    },
    hide() { bubble.hidden = true; },
    get visible() { return !bubble.hidden; },
  };
};
