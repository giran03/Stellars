<script lang="ts">
  import StudioSelect from '../components/StudioSelect.svelte';
  import EditableValue from '../components/EditableValue.svelte';
  import ConfirmModal from '../components/ConfirmModal.svelte';
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import './mock.css';
  import StudioTooltips from '../components/StudioTooltips.svelte';
  import AppIcon from '../components/AppIcon.svelte';
  import { computeBiquadCoeffs, evaluateBandMagnitudeDb, EQ_PRESETS, computeChatMixGains } from '../dspMath';
  import type { EqBandConfig } from '../types';

  type Channel = { name: string; icon: string; color: string; volume: number; mute: boolean; solo: boolean; gain: number };
  let channels = $state<Channel[]>([
    { name: 'Game', icon: 'game', color: '#b7a3ff', volume: 82, mute: false, solo: false, gain: 0 },
    { name: 'Chat', icon: 'chat', color: '#7be1d1', volume: 68, mute: false, solo: false, gain: 0 },
    { name: 'Media', icon: 'music', color: '#a1baff', volume: 45, mute: false, solo: false, gain: 0 },
    { name: 'Aux', icon: 'aux', color: '#edc990', volume: 60, mute: false, solo: false, gain: 0 },
    { name: 'Mic', icon: 'mic', color: '#f0a7c6', volume: 76, mute: false, solo: false, gain: 0 },
  ]);
  let apps = $state([
    { name: 'Counter-Strike 2', detail: 'cs2.exe', bus: 'Game', initials: 'CS', color: '#d9bc85' },
    { name: 'Discord', detail: 'discord.exe', bus: 'Chat', initials: 'DC', color: '#a9aff5' },
    { name: 'Spotify', detail: 'spotify.exe', bus: 'Media', initials: 'SP', color: '#8cd6ac' },
    { name: 'OBS Studio', detail: 'obs64.exe', bus: 'Aux', initials: 'OB', color: '#d0d0d8' },
    { name: 'Google Chrome', detail: 'chrome.exe', bus: 'Unrouted', initials: 'CH', color: '#b4c9de' },
  ]);
  let selected = $state('Game');
  let playing = $state(true);
  let visible = $state(true);
  let tick = $state(0);
  let query = $state('');
  let chatMix = $state(0);
  let master = $state(85);
  let streamLevel = $state(80);
  let streamMuted = $state(false);
  let streamEnabled = $state(false);
  let streamOpen = $state(false);
  let streamSources = $state(['Game', 'Chat', 'Mic']);
  let draftSources = $state<string[]>([]);
  let streamDestination = $state('');
  let draftDestination = $state('');
  let overrides = $state<Record<string,string>>({});
  let draftOverrides = $state<Record<string,string>>({});
  let streamSearch = $state('');
  let streamRefreshing = $state(false);
  let draftStreamEnabled = $state(false);
  let streamSaving = $state(false);
  let streamError = $state('');
  let silentSink = $state('Silent routing sink · Demo');
  let virtualMic = $state('Virtual mic bridge · Demo');
  let routingRules = $state<Record<string,string>>({'cs2.exe':'Game','discord.exe':'Chat','spotify.exe':'Media','obs64.exe':'Aux'});
  let bandIndex = $state(0);
  let draggingBand = $state<number|null>(null);
  let hardwareOpen = $state(false);
  let output = $state('Studio headphones');
  let micDevice = $state('Shure SM7B');
  let filter = $state('All apps');
  let dragApp = $state('');
  let overBus = $state('');
  let notice = $state('');
  let toastTimer: ReturnType<typeof setTimeout>;
  const frequencies = ['31', '63', '125', '250', '500', '1k', '2k', '4k', '8k', '16k'];
  const initial = [...EQ_PRESETS.find(p=>p.id==='fps')!.gains];
  let curves = $state<Record<string,number[]>>(Object.fromEntries(channels.map(c=>[c.name,[...initial]])));
  let preset = $state<Record<string,string>>(Object.fromEntries(channels.map(c=>[c.name,'FPS footsteps'])));
  let gates = $state<Record<string,boolean>>({Game:false, Chat:true, Media:false, Aux:false, Mic:true});
  let comps = $state<Record<string,boolean>>({Game:true, Chat:false, Media:false, Aux:false, Mic:true});
  let eqEnabled = $state<Record<string,boolean>>({Game:true, Chat:true, Media:true, Aux:true, Mic:true});
  let thresholds = $state<Record<string,number>>(Object.fromEntries(channels.map(c=>[c.name,-42])));
  let ratios = $state<Record<string,number>>(Object.fromEntries(channels.map(c=>[c.name,3])));
  let eqOptions = $state<Record<string,Omit<EqBandConfig,'gainDb'>[]>>(Object.fromEntries(channels.map(c=>[c.name,[31.25,62.5,125,250,500,1000,2000,4000,8000,16000].map((freq,i)=>({freq,q:i===0||i===9?.707:1.414,enabled:true,type:i===0?'LowShelf':i===9?'HighShelf':'Peaking'}))])));
  let advanced = $state<Record<string,Record<string,number>>>(Object.fromEntries(channels.map(c=>[c.name,{gateRatio:4,gateAttack:2,gateRelease:80,compThreshold:-18,compAttack:10,compRelease:120,makeup:0,knee:6}])));
  const advancedParams = [{key:'gateRatio',label:'Gate ratio',min:1,max:20,step:.5,unit:':1'},{key:'gateAttack',label:'Gate attack',min:.5,max:50,step:.5,unit:'ms'},{key:'gateRelease',label:'Gate release',min:10,max:500,step:5,unit:'ms'},{key:'compThreshold',label:'Compressor threshold',min:-60,max:0,step:1,unit:'dB'},{key:'compAttack',label:'Compressor attack',min:.5,max:100,step:.5,unit:'ms'},{key:'compRelease',label:'Compressor release',min:10,max:1000,step:5,unit:'ms'},{key:'makeup',label:'Makeup gain',min:0,max:18,step:.5,unit:'dB'},{key:'knee',label:'Soft knee',min:0,max:12,step:.5,unit:'dB'}];
  const active = $derived(channels.find(c=>c.name===selected)!);
  const filtered = $derived(apps.filter(a=>(a.name+' '+a.detail).toLowerCase().includes(query.toLowerCase()) && (filter==='All apps' || a.bus==='Unrouted')));
  const eqPath = $derived.by(()=>{
    if (!eqEnabled[selected]) return 'M 30 90 L 660 90';
    const coeffs=eqOptions[selected].map((b,i)=>computeBiquadCoeffs({...b,gainDb:curves[selected][i]}));
    return Array.from({length:181},(_,i)=>{
      const freq=20*Math.pow(1000,i/180);
      const gain=coeffs.reduce((sum,c)=>sum+evaluateBandMagnitudeDb(c,freq),0);
      return `${i===0?'M':'L'} ${30+i*3.5} ${90-Math.max(-12,Math.min(12,gain))*7}`;
    }).join(' ');
  });
  const streamCount = $derived(apps.filter(a=>overrides[a.name]==='Include' || (overrides[a.name]!=='Exclude' && streamSources.includes(a.bus))).length + (streamSources.includes('Mic')?1:0));
  const streamStatus = $derived(!streamEnabled?'Off':streamMuted?'Muted':!destinations.includes(streamDestination)?'Output unavailable':'Preview');
  const streamPeak = $derived(streamEnabled&&!streamMuted&&streamStatus==='Preview'&&streamCount>0 ? Math.min(100,(38+Math.sin(tick*.55)*12)*streamLevel/100):0);
  const monitorGains = $derived(computeChatMixGains(chatMix/100));
  const streamApps = $derived(apps.filter(a=>(a.name+' '+a.detail).toLowerCase().includes(streamSearch.toLowerCase())));
  const destinations = $derived(['CABLE-A Input · Demo','Recording cable · Demo'].filter(d=>d!==output&&d!==silentSink&&d!==virtualMic));

  function notify(message:string) { notice=message; clearTimeout(toastTimer); toastTimer=setTimeout(()=>notice='',2800); }
  function route(name:string,bus:string) { const app=apps.find(a=>a.name===name); if(app) {app.bus=bus;if(bus==='Unrouted')delete routingRules[app.detail];else routingRules[app.detail]=bus; notify(bus==='Unrouted'?`${name} returned to unrouted`:`${name} routed to ${bus}`);} }
  function choosePreset(value:string) { const id=({'Flat':'flat','Voice clarity':'voice','Bass & cinema':'cinema','Broadcast mic':'broadcast','FPS footsteps':'fps'} as Record<string,string>)[value];const p=EQ_PRESETS.find(p=>p.id===id);if(!p)return;preset[selected]=value;curves[selected]=[...p.gains];eqOptions[selected]=eqOptions[selected].map((b,i)=>({...b,enabled:true,q:p.qs?.[i]??b.q})); }
  function resetMockEqDefaults() {
    const defMap: Record<string,string> = {Game:'FPS footsteps', Chat:'Voice clarity', Media:'Bass & cinema', Mic:'Broadcast mic', Aux:'Flat'};
    const def = defMap[selected] || 'Flat';
    choosePreset(def);
    eqEnabled[selected] = true;
    notify(`${selected} EQ reset to default (${def})`);
  }
  function openStream() { draftSources=[...streamSources]; draftDestination=streamDestination; draftOverrides={...overrides};draftStreamEnabled=streamEnabled;streamSearch='';streamError=''; streamOpen=!streamOpen; }
  function toggleSource(name:string) { draftSources=draftSources.includes(name)?draftSources.filter(s=>s!==name):[...draftSources,name]; }
  async function saveStream() {if(draftStreamEnabled&&!destinations.includes(draftDestination)){streamError='Choose an available dedicated output before enabling Stream.';return;}streamSaving=true;await new Promise(resolve=>setTimeout(resolve,240));streamSources=[...draftSources]; streamDestination=draftDestination; overrides=Object.fromEntries(Object.entries(draftOverrides).filter(([,v])=>v==='Include'||v==='Exclude'));streamEnabled=draftStreamEnabled;streamError='';streamSaving=false; streamOpen=false; notify('Stream mix settings saved');}
  async function refreshStreamDevices(){streamRefreshing=true;await new Promise(resolve=>setTimeout(resolve,350));streamRefreshing=false;notify('Demo recording devices refreshed');}
  let confirmOpen = $state(false);
  function promptResetRoutes(){confirmOpen = true;}
  function handleConfirmResetRoutes(){confirmOpen = false; resetRoutes();}
  function resetRoutes(){apps.forEach(a=>a.bus='Unrouted');routingRules={};notify('Demo apps restored to system default');}
  function moveBand(e:PointerEvent){if(draggingBand===null)return;const r=e.currentTarget instanceof SVGSVGElement?e.currentTarget.getBoundingClientRect():null;if(!r)return;const x=(e.clientX-r.left)/r.width*690;const y=(e.clientY-r.top)/r.height*180;eqOptions[selected][draggingBand].freq=Math.round(Math.max(20,Math.min(20000,20*Math.pow(1000,(x-30)/630))));curves[selected][draggingBand]=Math.round(Math.max(-12,Math.min(12,(90-y)/7))*2)/2;preset[selected]='Custom';}
  function meterLevel(i:number,col:number) {if(channels[i].mute || (channels.some(c=>c.solo)&&!channels[i].solo))return 0; const balance=i<2?monitorGains[i]:1;return Math.max(0,Math.min(18,Math.round((9+Math.sin(tick*.6+i*1.8+col*.65)*4+Math.sin(col*1.7+i)*3)*channels[i].volume/85*balance*Math.pow(10,channels[i].gain/20))));}
  function db(value:number) {return value===0?'−∞':(20*Math.log10(value/100)).toFixed(1);}
  onMount(()=>{const timer=setInterval(()=>{if(playing&&visible)tick++},110); const visibility=()=>visible=!document.hidden; document.addEventListener('visibilitychange',visibility);return()=>{clearInterval(timer);clearTimeout(toastTimer);document.removeEventListener('visibilitychange',visibility);};});
</script>

<svelte:head><title>Stellars — Studio mockup</title><meta name="theme-color" content="#111113"/></svelte:head>

<StudioTooltips/><div class="studio">
  <aside class="rail" aria-label="Console navigation">
    <a class="brand-mark" href="?mock" aria-label="Stellars mockup" data-placement="right"><svg viewBox="0 0 256 256" width="32" height="32" fill="none" aria-hidden="true"><path fill-rule="evenodd" fill="#b8a1fa" d="M128 20C128 82 174 128 236 128C174 128 128 174 128 236C128 174 82 128 20 128C82 128 128 82 128 20ZM128 86L170 128 128 170 86 128Z"/><polygon points="128,108 148,128 128,148 108,128" fill="#edc990"/><path fill="#87ddcf" d="M174 62A96 96 0 0 1 194 82L185 91A84 84 0 0 0 165 71ZM188 48A116 116 0 0 1 208 68L199 77A104 104 0 0 0 179 57ZM82 194A96 96 0 0 1 62 174L71 165A84 84 0 0 0 91 185ZM68 208A116 116 0 0 1 48 188L57 179A104 104 0 0 0 77 199Z"/><circle cx="70" cy="70" r="7" fill="#9bd6ff"/><circle cx="186" cy="186" r="7" fill="#9bd6ff"/></svg></a>
    <nav>
      <a href="#mixer" class="rail-button current" aria-label="Mixer" data-placement="right"><Icon name="mix"/></a>
      <a href="#routing" class="rail-button" aria-label="App routing" data-placement="right"><Icon name="route"/></a>
      <a href="#stream" class="rail-button" aria-label="Stream output" data-placement="right"><Icon name="broadcast"/></a>
      <a href="#equalizer" class="rail-button" aria-label="Equalizer" data-placement="right"><Icon name="wave"/></a>
    </nav>
    <div class="rail-bottom"><button class="rail-button" class:chosen={hardwareOpen} onclick={()=>hardwareOpen=!hardwareOpen} aria-label="Hardware settings" aria-expanded={hardwareOpen} data-placement="right"><Icon name="settings"/></button><span class="avatar" data-placement="right" title="Local demo workspace">S</span></div>
  </aside>

  <div class="workspace">
    <header class="console-header"><a href="?mock" class="wordmark">stellars<span class="wordmark-dot">®</span></a><div class="header-center"><span class="status-dot"></span>Demo workspace<span class="header-separator"></span><span>48 kHz <span class="muted">/</span> Stereo</span></div><div class="header-actions"><button class="header-reset-button" aria-label="Reset routes" onclick={promptResetRoutes}><Icon name="reset" size={15}/></button><button class="hardware-button" onclick={()=>hardwareOpen=!hardwareOpen} aria-expanded={hardwareOpen} aria-label="Audio settings"><Icon name="headphones" size={16}/><span>Audio Settings</span><Icon name="down" size={14}/></button></div></header>
    {#if hardwareOpen}
      <section class="hardware-drawer panel" aria-label="Hardware configuration"><div><h2>Hardware I/O</h2><p>Demo devices · no system audio is changed.</p></div><label>Monitor output<StudioSelect bind:value={output} aria-label="Monitor output"><option>Studio headphones</option><option>Desktop speakers</option><option value="Unavailable device">Unavailable device</option></StudioSelect></label><label>Silent routing sink<StudioSelect bind:value={silentSink} aria-label="Silent routing sink"><option>Silent routing sink · Demo</option><option>CABLE-A Input · Demo</option><option>None</option></StudioSelect></label><label>Virtual microphone output<StudioSelect bind:value={virtualMic} aria-label="Virtual microphone output"><option>Virtual mic bridge · Demo</option><option>Recording cable · Demo</option><option>Disabled</option></StudioSelect></label><label>Microphone<StudioSelect bind:value={micDevice}><option>Shure SM7B</option><option>USB microphone</option></StudioSelect></label><button class="icon-button" aria-label="Close hardware settings" onclick={()=>hardwareOpen=false}><Icon name="close"/></button>{#if output==='Unavailable device'}<p class="device-warning" role="alert">Device unavailable. Choose Studio headphones or Desktop speakers to continue.</p>{/if}</section>
    {/if}

    <div class="page-heading"><div><h1>Your mix, in focus<span>.</span></h1><p>A little less noise. A lot more you.</p></div><div class="heading-actions"><span class="preview-label">Interactive preview</span><button class="quiet-button" onclick={()=>playing=!playing} aria-pressed={playing}><Icon name={playing?'pause':'play'} size={14}/>{playing?'Pause meters':'Play meters'}</button></div></div>

    <div class="console-grid">
      <section id="mixer" class="mixer-area" aria-label="Five-bus mixer">
        <div class="section-heading"><h2>Mixer <span class="small-count">5 channels</span></h2><span class="section-note">Your sound. Your balance.</span></div>
        <div class="channel-grid">
          {#each channels as channel,i}
            <section class="channel" class:active-channel={selected===channel.name} class:muted-channel={channel.mute} class:drop-active={overBus===channel.name} style={`--channel-color:${channel.color};--channel-index:${i}`} aria-label={`${channel.name} channel`}
              ondragover={(e)=>{e.preventDefault();overBus=channel.name;}} ondragleave={()=>overBus=''} ondrop={(e)=>{e.preventDefault();route(dragApp,channel.name);overBus='';dragApp='';}}>
              <button class="channel-title" onclick={()=>selected=channel.name} aria-pressed={selected===channel.name}><span class="channel-symbol"><Icon name={channel.icon}/></span><span>{channel.name}</span><span class="selected-dot"></span></button>
              <div class="channel-sources">{channel.name==='Mic'?micDevice:apps.filter(a=>a.bus===channel.name).map(a=>a.name).join(', ')||'Drop an app here'}</div>
              <div class="meter-matrix" role="img" aria-label={`${channel.name} simulated audio activity`}>
                {#each Array(12) as _,col}<div class="dot-column">{#each Array(18) as _,row}<i class:lit={17-row<meterLevel(i,col)} class:peak={row<3}></i>{/each}</div>{/each}
              </div>
              <label class="pre-gain">Pre-gain <span><input type="number" min="-12" max="12" step=".5" bind:value={channel.gain} aria-label={`${channel.name} pre-gain`}/> dB</span></label><div class="channel-level"><EditableValue value={channel.volume>0?20*Math.log10(channel.volume/100):-60} min={-60} max={3.521825} step={.1} unit="dB" label={channel.name+' level'} display={db(channel.volume)+' dB'} allowSilence oncommit={v=>channel.volume=Math.pow(10,v/20)*100} onsilence={()=>channel.volume=0}/><span class="activity-label">{channel.mute?'Muted':channel.solo?'Solo':playing?'Peak':'Paused'}</span></div>
              <div class="fader-area"><div class="fader-ticks"><span style="top:calc(8px + (100% - 16px)*.0583)">+3</span><span style="top:calc(8px + (100% - 16px)/3)">0</span><span style="top:calc(100% - 8px)">−∞</span></div><div class="fader-track"><div class="fader-fill" style={`height:${channel.volume/1.5}%`}></div><div class="fader-unity"></div><input type="range" min="0" max="150" step="1" bind:value={channel.volume} aria-label={`${channel.name} volume`} orient="vertical" ondblclick={()=>{channel.volume=100;notify(`${channel.name} reset to 0 dB`);}} onwheel={(e)=>{e.preventDefault();channel.volume=Math.max(0,Math.min(150,channel.volume+(e.deltaY<0?1:-1)));}}/></div><div class="stereo-bars"><i style={`height:${channel.mute?0:meterLevel(i,3)*4}%`}></i><i style={`height:${channel.mute?0:meterLevel(i,8)*4}%`}></i></div></div>
              <div class="channel-buttons"><button class:engaged={channel.solo} aria-pressed={channel.solo} aria-label={`Solo ${channel.name}`} onclick={()=>channel.solo=!channel.solo}>S</button><button class:engaged={channel.mute} aria-pressed={channel.mute} aria-label={`Mute ${channel.name}`} onclick={()=>channel.mute=!channel.mute}><Icon name={channel.mute?'mute':'volume'} size={16}/></button></div>
              <button class="channel-edit" onclick={()=>selected=channel.name}>{selected===channel.name?'Editing channel':'Edit channel'}<Icon name="chevron" size={12}/></button>
            </section>
          {/each}
        </div>
        <div class="mix-bottom">
          <section class="chatmix panel"><div class="compact-title"><Icon name="mix" size={17}/><h3>ChatMix</h3><button class="text-button" onclick={()=>chatMix=0} aria-label="Reset ChatMix">Reset</button></div><div class="balance-labels"><span>Game</span><span class="balance-value"><EditableValue value={chatMix} min={-100} max={100} step={1} unit="%" label="ChatMix balance" display={chatMix===0?'Perfect balance':Math.abs(chatMix)+'% toward '+(chatMix<0?'Game':'Chat')} oncommit={v=>chatMix=v}/></span><span>Chat</span></div><input class="horizontal-range chat-range" type="range" min="-100" max="100" bind:value={chatMix} aria-label="Game and Chat balance" ondblclick={()=>chatMix=0}/></section>
          <section class="master-panel"><div class="master-heading"><span><Icon name="headphones" size={18}/>Monitor output</span><strong><EditableValue value={master} min={0} max={150} step={1} unit="%" label="Master monitor level" oncommit={v=>master=v}/></strong></div><input class="horizontal-range" type="range" min="0" max="150" bind:value={master} aria-label="Master monitor volume"/><div class="master-footer"><span>{output}</span><span>Limiter −0.1 dBFS</span></div></section>
        </div>
      </section>

      <section id="routing" class="routing panel" aria-label="App routing" ondragover={(e)=>e.preventDefault()} ondrop={(e)=>{e.preventDefault();if(e.target===e.currentTarget)route(dragApp,'Unrouted');}}><div class="routing-heading"><h2>App routing</h2><span class="count-badge">{apps.length}</span><button class="icon-button" aria-label="Refresh demo apps" onclick={()=>notify('Demo app list is up to date')}><Icon name="reset" size={16}/></button></div><p class="routing-intro">Give every app a place in your mix.</p><label class="search"><Icon name="search" size={16}/><input placeholder="Find an app…" bind:value={query} aria-label="Search audio apps"/></label><div class="filter-row">{#each ['All apps','Unrouted','Rules'] as name}<button class:filter-active={filter===name} aria-pressed={filter===name} onclick={()=>filter=name}>{name}{#if name==='Unrouted'}<span>{apps.filter(a=>a.bus==='Unrouted').length}</span>{/if}</button>{/each}</div>
        <div class="app-list">{#if filter==='Rules'}{#each Object.entries(routingRules).filter(([exe])=>exe.includes(query.toLowerCase())) as [exe,bus]}<div class="app-row rule-row"><Icon name="route" size={17}/><div class="app-details"><strong>{exe}</strong><span>Auto-route · {bus}</span></div><button class="icon-button" aria-label={'Remove '+exe+' routing rule'} onclick={()=>{delete routingRules[exe];notify('Demo routing rule removed');}}><Icon name="close" size={14}/></button></div>{:else}<div class="empty-state"><strong>No saved routes</strong><p>Assign an app to create a demo routing rule.</p></div>{/each}{:else}{#each filtered as app}<div class="app-row" role="group" aria-label={app.name} draggable="true" ondragstart={()=>dragApp=app.name} ondragend={()=>{dragApp='';overBus='';}}><AppIcon exeName={app.detail} displayName={app.name} size={30} color={channels.find(c=>c.name===app.bus)?.color||app.color}/><div class="app-details"><strong>{app.name}</strong><span>{app.detail}</span></div><label class="route-select" style={`color:${channels.find(c=>c.name===app.bus)?.color||'#b8b8c1'}`}><span class="sr-only">Route {app.name}</span><StudioSelect value={app.bus} onchange={(e)=>route(app.name,e.currentTarget.value)}>{#each ['Game','Chat','Media','Aux','Unrouted'] as bus}<option>{bus}</option>{/each}</StudioSelect><Icon name="down" size={11}/></label></div>{:else}<div class="empty-state"><Icon name="search" size={26}/><strong>{query?'No apps found':'All apps have a home'}</strong><p>{query?'Try a different name or clear your search.':'Choose All apps to adjust your routes.'}</p><button class="quiet-button" onclick={()=>{query='';filter='All apps';}}>Show all apps</button></div>{/each}{/if}</div>
        <div class="routing-tip"><Icon name="info" size={15}/><p>Drag an app onto a channel, or choose its destination above.</p></div>
        <div class="routing-footer"><span><span class="status-dot"></span>{apps.filter(a=>a.bus!=='Unrouted').length} apps routed</span><span>Demo data</span></div>
      </section>

      <section id="stream" class="stream-panel panel" aria-label="Stream recording output"><div class="stream-summary"><span class="stream-icon"><Icon name="broadcast" size={24}/></span><div class="stream-title"><h2>Stream<span class="stream-state" class:enabled={streamEnabled}>{streamStatus}</span></h2><p>A dedicated mix for your audience · Independent from your headphones</p></div><div class="stream-sources"><span>{streamCount} sources · {Object.keys(overrides).length} app overrides</span><small>{streamDestination||'Choose an output to get started'}</small></div><label class="stream-level"><span>Level <strong><EditableValue value={streamLevel} min={0} max={150} step={1} unit="%" label="Stream level" oncommit={v=>streamLevel=v}/> · {db(streamLevel)} dB</strong></span><input class="horizontal-range" type="range" min="0" max="150" bind:value={streamLevel} aria-label="Stream output level"/><div class="stream-stereo" role="meter" aria-label="Stream stereo peak" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(streamPeak)}><i style={`width:${streamPeak}%`}></i><i style={`width:${streamPeak*.88}%`}></i></div></label><button class="icon-button" class:chosen={streamMuted} aria-label="Mute stream output" aria-pressed={streamMuted} onclick={()=>streamMuted=!streamMuted}><Icon name={streamMuted?'mute':'volume'} size={18}/></button><button class="quiet-button" aria-expanded={streamOpen} onclick={openStream}>Mix settings<Icon name={streamOpen?'down':'chevron'} size={13}/></button><button class="switch" class:on={streamEnabled} disabled={!streamDestination} aria-label="Enable stream output" aria-pressed={streamEnabled} title={streamDestination?'Toggle demo stream':'Choose a destination in Mix settings first'} onclick={()=>streamEnabled=!streamEnabled}><span></span></button></div>
      {#if streamOpen}<div class="stream-settings" aria-busy={streamSaving}><div><h3>Included channels</h3><div class="source-chips">{#each channels as channel}<button class:included={draftSources.includes(channel.name)} aria-pressed={draftSources.includes(channel.name)} onclick={()=>toggleSource(channel.name)}>{#if draftSources.includes(channel.name)}<Icon name="check" size={13}/>{/if}{channel.name}</button>{/each}</div><p>App audio is captured before monitor faders and DSP. Mic uses its processed signal.</p><h3>App overrides</h3><label class="search stream-search"><Icon name="search" size={15}/><input type="search" bind:value={streamSearch} aria-label="Search stream apps" placeholder="Find an app or executable…"/></label><div class="override-grid">{#each streamApps as app}<label><span>{app.name}<small>{app.detail} · {(draftOverrides[app.name]==='Include'||(draftOverrides[app.name]!=='Exclude'&&draftSources.includes(app.bus)))?'Included':'Excluded'}</small></span><StudioSelect bind:value={draftOverrides[app.name]} aria-label={`${app.name} stream override`}><option value={undefined}>Follow category</option><option value="Include">Always include</option><option value="Exclude">Always exclude</option></StudioSelect></label>{:else}<p>No apps match your search.</p>{/each}</div><p class="capture-note">Each app is captured once, up to 32 active apps. App overrides take precedence over category selections.</p></div><div class="stream-destination"><label>Recording destination<StudioSelect bind:value={draftDestination} aria-label="Recording destination"><option value="">Select an output…</option>{#each destinations as destination}<option>{destination}</option>{/each}<option>Unavailable output</option></StudioSelect></label><button class="text-button refresh-devices" disabled={streamRefreshing} onclick={refreshStreamDevices}>{streamRefreshing?'Refreshing…':'Refresh devices'}</button>{#if draftDestination&&!destinations.includes(draftDestination)}<p class="device-warning" role="alert">This output is disconnected or already used elsewhere. Choose another cable.</p>{/if}<label class="enable-stream"><input type="checkbox" bind:checked={draftStreamEnabled}/>Enable Stream output</label><p>In OBS or your recorder, add the cable’s paired recording endpoint as an audio input. Use a separate cable from the silent sink and virtual mic; capture the mix once to avoid doubled audio.</p><p>Device names and audio activity are simulated. Changes apply when saved.</p>{#if streamError}<p class="device-warning" role="alert">{streamError}</p>{/if}<div class="save-row"><button class="quiet-button" onclick={()=>streamOpen=false}>Cancel</button><button class="primary-button" disabled={streamSaving} onclick={saveStream}>{streamSaving?'Saving…':'Save settings'}</button></div></div></div>{/if}</section>

      <section id="equalizer" class="eq-panel panel"><div class="eq-heading"><div><h2>Shape your sound</h2><p><span style={`color:${active.color}`}>{selected}</span><span class="middot">·</span>10-band parametric EQ</p></div><div class="eq-actions"><div class="eq-master-toggle" data-tooltip={eqEnabled[selected]?`Bypass ${selected} EQ`:`Enable ${selected} EQ`}><span class="eq-toggle-label">{eqEnabled[selected]?'EQ':'BYP'}</span><button type="button" class="switch" class:on={eqEnabled[selected]} aria-pressed={eqEnabled[selected]} aria-label={`${selected} Equalizer toggle`} onclick={()=>{eqEnabled[selected]=!eqEnabled[selected];notify(eqEnabled[selected]?`${selected} EQ enabled`:`${selected} EQ bypassed`);}}><span></span></button></div><StudioSelect value={preset[selected]} aria-label="EQ preset" onchange={(e)=>choosePreset(e.currentTarget.value)}><option>FPS footsteps</option><option>Voice clarity</option><option>Bass &amp; cinema</option><option>Broadcast mic</option><option>Flat</option><option disabled>Custom</option></StudioSelect><button class="icon-button" aria-label="Reset to default EQ" data-tooltip="Reset EQ to default" onclick={resetMockEqDefaults}><Icon name="reset" size={16}/></button><button type="button" class="btn-flat" onclick={()=>choosePreset('Flat')} data-tooltip="Reset all 10 bands to 0.0 dB flat response">Flatten 0 dB</button></div></div>
        <div class="eq-graph"><div class="eq-axis"><span>+12</span><span>0 dB</span><span>−12</span></div><svg class="eq-svg" viewBox="0 0 690 180" preserveAspectRatio="none" role="group" aria-label={`${selected} equalizer response preview`} onpointermove={moveBand} onpointerup={()=>draggingBand=null} onpointercancel={()=>draggingBand=null}><defs><linearGradient id="eq-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color={active.color} stop-opacity=".18"/><stop offset="100%" stop-color={active.color} stop-opacity="0"/></linearGradient></defs>{#each [6,48,90,132,174] as y}<line x1="0" y1={y} x2="690" y2={y} stroke="#ffffff" stroke-opacity={y===90?'.14':'.045'} stroke-dasharray={y===90?'0':'3 5'}/>{/each}{#each Array(10) as _,i}<line x1={30+i*70} y1="0" x2={30+i*70} y2="180" stroke="white" stroke-opacity=".035"/>{/each}<path d={`${eqPath} L 660 180 L 30 180 Z`} fill="url(#eq-fill)"/><path d={eqPath} fill="none" stroke={active.color} stroke-width="2.5" stroke-linejoin="round"/>{#each curves[selected] as gain,i}<circle cx={30+Math.log(eqOptions[selected][i].freq/20)/Math.log(1000)*630} cy={90-gain*7} r={bandIndex===i?5:4} fill={eqOptions[selected][i].enabled?active.color:"#1c1927"} stroke={active.color} stroke-width="2" role="button" tabindex="0" aria-label={`Select EQ band ${i+1}`} onkeydown={(e)=>{if(e.key==="Enter"||e.key===" "){e.preventDefault();bandIndex=i;}}} onpointerdown={(e)=>{e.preventDefault();bandIndex=i;draggingBand=i;(e.currentTarget.ownerSVGElement)?.setPointerCapture(e.pointerId);}}/>{/each}</svg></div>
        <div class="eq-bands">{#each curves[selected] as gain,i}<label><span>{frequencies[i]}</span><input type="range" min="-12" max="12" step=".5" value={gain} aria-label={`${selected} EQ ${frequencies[i]} Hz gain`} oninput={(e)=>{curves[selected][i]=+e.currentTarget.value;preset[selected]='Custom';}}/><small><EditableValue value={gain} min={-12} max={12} step={.5} unit="dB" label={selected+' EQ band '+(i+1)+' gain'} display={(gain>0?'+':'')+gain} oncommit={v=>{curves[selected][i]=v;preset[selected]='Custom';}}/></small></label>{/each}</div><details class="band-details"><summary>Band controls <span>Frequency · Q · filter type</span></summary><div class="band-fields"><label>Band<StudioSelect bind:value={bandIndex} aria-label="Selected EQ band">{#each frequencies as freq,i}<option value={i}>{i+1} · {freq} Hz</option>{/each}</StudioSelect></label><label>Frequency / Hz<input type="number" min="20" max="20000" step="1" bind:value={eqOptions[selected][bandIndex].freq} aria-label="EQ band frequency" oninput={()=>preset[selected]='Custom'}/></label><label>Q<input type="number" min=".1" max="10" step=".1" bind:value={eqOptions[selected][bandIndex].q} aria-label="EQ band Q" oninput={()=>preset[selected]='Custom'}/></label><label>Filter<StudioSelect bind:value={eqOptions[selected][bandIndex].type} aria-label="EQ filter type" onchange={()=>preset[selected]='Custom'}>{#each ['LowShelf','Peaking','HighShelf','LowPass','HighPass'] as type}<option>{type}</option>{/each}</StudioSelect></label><label class="band-enabled"><input type="checkbox" bind:checked={eqOptions[selected][bandIndex].enabled} onchange={()=>preset[selected]='Custom'}/>Enabled</label></div></details><div class="eq-footer"><span>Frequency / Hz</span><span>Drag a node or adjust a band</span></div>
      </section>

      <section class="dynamics panel" aria-label="Channel dynamics"><div class="section-heading"><h2>Fine-tune</h2><span style={`color:${active.color}`}>{selected}</span></div><div class="dynamics-row"><span class="dynamics-icon"><Icon name="wave" size={18}/></span><div><h3>Noise gate</h3><p>Keep the quiet parts quiet.</p></div><button class="switch" class:on={gates[selected]} aria-label={`${selected} noise gate`} aria-pressed={gates[selected]} onclick={()=>gates[selected]=!gates[selected]}><span></span></button></div><label class="dynamics-control"><span>Threshold<strong><EditableValue value={thresholds[selected]} min={-80} max={0} step={1} unit="dB" label={selected+' gate Threshold'} oncommit={v=>thresholds[selected]=v}/></strong></span><input class="horizontal-range" type="range" min="-80" max="0" bind:value={thresholds[selected]} disabled={!gates[selected]} aria-label={`${selected} gate threshold`}/></label><div class="dynamics-divider"></div><div class="dynamics-row"><span class="dynamics-icon"><Icon name="settings" size={18}/></span><div><h3>Compressor</h3><p>Smooth out the unexpected.</p></div><button class="switch" class:on={comps[selected]} aria-label={`${selected} compressor`} aria-pressed={comps[selected]} onclick={()=>comps[selected]=!comps[selected]}><span></span></button></div><label class="dynamics-control"><span>Ratio<strong><EditableValue value={ratios[selected]} min={1} max={20} step={.5} unit=":1" label={selected+' compressor Ratio'} oncommit={v=>ratios[selected]=v}/></strong></span><input class="horizontal-range" type="range" min="1" max="20" step=".5" bind:value={ratios[selected]} disabled={!comps[selected]} aria-label={selected+' compressor ratio'}/></label><details class="advanced-dynamics"><summary>More dynamics controls</summary><div class="advanced-fields">{#each advancedParams as param}<label class="dynamics-control"><span>{param.label}<strong><EditableValue value={advanced[selected][param.key]} min={param.min} max={param.max} step={param.step} unit={param.unit} label={selected+' '+param.label} oncommit={v=>advanced[selected][param.key]=v}/></strong></span><input class="horizontal-range" type="range" min={param.min} max={param.max} step={param.step} bind:value={advanced[selected][param.key]} aria-label={selected+' '+param.label} disabled={param.key.startsWith('gate')?!gates[selected]:!comps[selected]}/></label>{/each}</div></details></section>


    </div>
    <footer class="workspace-footer"><span><span class="footer-star"><Icon name="wave" size={13}/></span> Made for the way you listen.</span><span>Design preview <span class="middot">·</span>Simulated audio, local controls</span><a href="/">Open current console<Icon name="arrow" size={13}/></a></footer>
  </div>
</div>
{#if notice}<div class="toast" role="status"><Icon name="check" size={17}/>{notice}</div>{/if}
{#if confirmOpen}
  <ConfirmModal
    open={confirmOpen}
    title="Reset All Routed Applications?"
    subtitle="Routing reset"
    message="This will clear all application routing and restore all active apps back to default system playback (Aux)."
    details={[
      "All custom application-to-bus routes will be cleared",
      "Active apps will revert to Windows default playback (Aux)",
      "Channel faders, volumes, and EQ curves will remain saved"
    ]}
    confirmLabel="Reset All Routes"
    cancelLabel="Keep My Routes"
    tone="warning"
    icon="reset"
    accentColor="#edc990"
    onConfirm={handleConfirmResetRoutes}
    onCancel={() => (confirmOpen = false)}
  />
{/if}





