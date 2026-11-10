<script lang="ts">
  import { onMount, tick } from 'svelte';
  let text=$state(''),shown=$state(false),left=$state(0),top=$state(0),side=$state('bottom');
  let arrowY=$state<number|null>(null);
  let target:HTMLElement|null=null;
  let surface=$state<HTMLDivElement>();
  const id=$props.id();
  const hints:[RegExp,string][]=[
    [/noise gate/i,'Noise gate: attenuates signals below its threshold to reduce background noise.'],
    [/compressor.*knee|soft knee/i,'Soft knee: the dB region where compression begins gradually.'],
    [/compressor.*makeup/i,'Makeup gain: boosts the signal after compression.'],
    [/gate.*threshold/i,'Gate threshold: quieter signals are attenuated. A higher threshold removes more background sound.'],
    [/compressor.*threshold/i,'Compressor threshold: signals above this level are reduced.'],
    [/ratio/i,'Ratio: how strongly signals beyond the threshold are attenuated.'],
    [/attack/i,'Attack: how quickly the processor reacts, in milliseconds.'],
    [/release/i,'Release: how quickly the processor returns to its resting gain.'],
    [/compressor/i,'Compressor: reduces loud peaks and narrows the dynamic range.'],
    [/solo /i,'Solo: hear this channel while temporarily silencing other monitor channels. Stream remains independent.'],
    [/stream send/i,'Mute this channel in Stream without changing its monitor or Mic output.'],
    [/stream volume fader/i,'Set this channel’s send to Stream independently of its monitor or Mic output. Double-click to reset to 0 dB.'],
    [/mute stream/i,'Mute the recording mix without changing what you hear in your headphones.'],
    [/mute /i,'Mute this monitor channel. The independent Stream mix is unchanged.'],
    [/pre-gain/i,'Pre-gain: trim the signal before EQ, gating, and compression.'],
    [/game and chat balance/i,'ChatMix: balance Game and Chat in your monitor mix. Negative favors Game; positive favors Chat.'],
    [/master monitor/i,'Monitor level: controls headphone or speaker volume independently of Stream.'],
    [/stream output volume/i,'Stream level: controls the independent recording mix.'],
    [/q bandwidth|band q|eq q/i,'Q controls filter width. Higher values affect a narrower frequency range.'],
    [/eq gain/i,'EQ gain: boost or cut the selected filter band, in decibels.'],
    [/frequency/i,'Frequency: the center or cutoff frequency of the selected EQ band, in Hz.'],
    [/topology|filter type/i,'Filter type: choose a shelf, peaking filter, low-pass, or high-pass response.'],
    [/hardware settings/i,'Configure the monitor output, microphone, silent routing sink, and virtual microphone bridge.'],
    [/remove.*routing rule/i,'Remove this saved auto-route rule and restore matching active apps to system playback.'],
    [/refresh audio apps/i,'Refresh the list of Windows applications currently playing audio.'],
    [/silent routing sink/i,'Silent sink: receives the original app audio to prevent duplicated playback.'],
    [/virtual microphone/i,'Virtual microphone: sends the processed mic signal to a separate virtual cable.'],
    [/equalizer graph/i,'Drag an EQ node to change frequency and gain. Scroll to change Q. Double-click a node to reset its gain.'],
    [/reset routes/i,'Restore routed applications to system playback and clear saved routing rules.'],
  ];
  function description(el:HTMLElement){if(el.dataset.tooltip)return el.dataset.tooltip;const label=el.getAttribute('aria-label')||el.dataset.studioTitle||'';const hint=hints.find(([pattern])=>pattern.test(label));if(el.getAttribute('role')==='combobox')return (hint?.[1]||label)+'. Current: '+(el.firstElementChild?.textContent||'not selected');return hint?.[1]||label;}
  function restore(){if(target){const ids=(target.getAttribute('aria-describedby')||'').split(' ').filter(v=>v&&v!==id);ids.length?target.setAttribute('aria-describedby',ids.join(' ')):target.removeAttribute('aria-describedby');}target=null;}
  function hide(){shown=false;restore();}
  function place(){if(!target||!surface)return;const r=target.getBoundingClientRect(),s=surface.getBoundingClientRect();const placement=target.dataset.placement||(target.closest('.rail, .rail-button')?'right':'auto');if((placement==='right'||(placement==='auto'&&target.closest('.rail'))&&r.right+s.width+16<innerWidth)){side='right';left=Math.min(r.right+10,innerWidth-s.width-8);const targetCenterY=r.top+r.height/2;top=Math.max(8,Math.min(targetCenterY-s.height/2,innerHeight-s.height-8));arrowY=Math.max(8,Math.min(targetCenterY-top-4,s.height-12));}else if(placement==='left'&&r.left-s.width-16>0){side='left';left=Math.max(8,r.left-s.width-10);const targetCenterY=r.top+r.height/2;top=Math.max(8,Math.min(targetCenterY-s.height/2,innerHeight-s.height-8));arrowY=Math.max(8,Math.min(targetCenterY-top-4,s.height-12));}else{left=Math.max(8,Math.min(r.left+(r.width-s.width)/2,innerWidth-s.width-8));side=r.bottom+s.height+16<innerHeight?'bottom':'top';top=side==='bottom'?r.bottom+8:Math.max(8,r.top-s.height-8);arrowY=null;}}
  function portal(node:HTMLElement){document.body.append(node);return{destroy(){node.remove();}};}
  onMount(()=>{
    let delay:ReturnType<typeof setTimeout>,dismiss:ReturnType<typeof setTimeout>;
    function harvest(root:ParentNode){root.querySelectorAll<HTMLElement>('.studio [title]').forEach(el=>{el.dataset.studioTitle=el.title;el.removeAttribute('title');});}
    harvest(document);const observer=new MutationObserver(()=>{if(document.body.classList.contains('has-active-modal')&&!target?.closest('.category-modal, [role="dialog"]')){hide();}harvest(document);});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['title','class']});
    async function show(el:HTMLElement){if(document.documentElement.dataset.tooltips==='off')return;if(document.body.classList.contains('is-dragging-app')||el.getAttribute('aria-expanded')==='true'||document.querySelector('.studio-select-menu'))return;if(document.body.classList.contains('has-active-modal')&&!el.closest('.category-modal, [role="dialog"]'))return;const next=description(el);if(!next)return;hide();target=el;text=next;const placement=el.dataset.placement||(el.closest('.rail, .rail-button')?'right':'auto');side=placement==='right'||(placement==='auto'&&el.closest('.rail'))?'right':'bottom';shown=true;const described=el.getAttribute('aria-describedby');el.setAttribute('aria-describedby',[described,id].filter(Boolean).join(' '));await tick();place();}
    function enter(e:Event){if(document.body.classList.contains('is-dragging-app'))return;const node=e.target as HTMLElement;if(document.body.classList.contains('has-active-modal')&&!node.closest('.category-modal, [role="dialog"], .studio-select-menu'))return;if(surface?.contains(node)){clearTimeout(dismiss);return;}const el=node.closest<HTMLElement>('[data-tooltip],[data-studio-title],button[aria-label],a[aria-label],input[type=range],canvas');if(!el||(!el.closest('.studio')&&!el.closest('.category-modal, [role="dialog"]'))||node.closest('.precise-value.editing')||el.getAttribute('aria-expanded')==='true')return;if(el===target)return;clearTimeout(delay);clearTimeout(dismiss);delay=setTimeout(()=>{if(el.isConnected)show(el);},80);}
    function leave(e:MouseEvent){const related=e.relatedTarget as Node|null;if(target?.contains(related)||surface?.contains(related))return;clearTimeout(delay);clearTimeout(dismiss);dismiss=setTimeout(hide,100);}
    function key(e:KeyboardEvent){if(e.key==='Escape'){clearTimeout(delay);clearTimeout(dismiss);hide();}}
    function pointerdown(){clearTimeout(delay);hide();}
    function onFocusOut(){clearTimeout(delay);queueMicrotask(hide);}
    document.addEventListener('pointerover',enter);document.addEventListener('pointerout',leave);document.addEventListener('focusin',enter);document.addEventListener('focusout',onFocusOut);document.addEventListener('pointerdown',pointerdown,true);document.addEventListener('keydown',key);document.addEventListener('scroll',pointerdown,true);window.addEventListener('resize',pointerdown);window.addEventListener('dragstart',pointerdown,true);window.addEventListener('drag',pointerdown,true);
    return()=>{observer.disconnect();clearTimeout(delay);clearTimeout(dismiss);restore();document.removeEventListener('pointerover',enter);document.removeEventListener('pointerout',leave);document.removeEventListener('focusin',enter);document.removeEventListener('focusout',onFocusOut);document.removeEventListener('pointerdown',pointerdown,true);document.removeEventListener('keydown',key);document.removeEventListener('scroll',pointerdown,true);window.removeEventListener('resize',pointerdown);window.removeEventListener('dragstart',pointerdown,true);window.removeEventListener('drag',pointerdown,true);};
  });
</script>

{#if shown}<div bind:this={surface} use:portal {id} role="tooltip" class="studio-tooltip" data-placement={side} style={`left:${left}px;top:${top}px;${arrowY!==null?`--arrow-y:${arrowY}px;`:''}`}>{text}</div>{/if}
