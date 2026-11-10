<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import type { HTMLSelectAttributes } from 'svelte/elements';
  import StudioIcon from './StudioIcon.svelte';
  type Props = Omit<HTMLSelectAttributes,'children'|'value'> & {children:Snippet;value?:string|number|null};
  let {children,value=$bindable(),...attributes}:Props=$props();
  let source:HTMLSelectElement;
  let trigger:HTMLButtonElement;
  let list=$state<HTMLDivElement>();
  let choices=$state<{label:string;disabled:boolean}[]>([]);
  let selected=$state(-1), highlighted=$state(-1), open=$state(false), name=$state('Choose an option');
  let left=$state(0), top=$state(0), width=$state(180), maxHeight=$state(300);
  let uid=$props.id();
  let search='',searchTimer:ReturnType<typeof setTimeout>;
  const disabled=$derived(Boolean(attributes.disabled));
  function refresh(){
    if(!source)return;
    choices=Array.from(source.options,o=>({label:o.label,disabled:o.disabled||!!o.closest('optgroup')?.disabled}));
    selected=source.selectedIndex;
    name=String(attributes['aria-label']||Array.from(source.labels??[],label=>{const copy=label.cloneNode(true) as HTMLElement;copy.querySelectorAll('select,.studio-select,.sr-only,svg').forEach(el=>el.remove());return copy.textContent?.trim()||label.querySelector('.sr-only')?.textContent||'';}).join(' ').trim()||'Choose an option');
    if(open&&(highlighted<0||highlighted>=choices.length))highlighted=selected;
  }
  function position(){
    if(!trigger)return;
    const r=trigger.getBoundingClientRect();
    width=Math.min(Math.max(130,r.width),Math.min(480,innerWidth-16));
    let desiredLeft = r.left;
    if (desiredLeft + width > innerWidth - 8) {
      desiredLeft = r.right - width;
    }
    left=Math.max(8,Math.min(desiredLeft,innerWidth-width-8));
    const below=innerHeight-r.bottom-16,above=r.top-16;
    maxHeight=Math.min(320,Math.max(below,above));
    const height=Math.min(list?.scrollHeight||choices.length*38+12,maxHeight);
    top=below>=height?r.bottom+8:Math.max(8,r.top-height-8);
  }
  async function show(){if(disabled)return;refresh();highlighted=choices[selected]&&!choices[selected].disabled?selected:choices.findIndex(c=>!c.disabled);position();open=true;await tick();position();reveal();}
  function reveal(){list?.querySelector<HTMLElement>(`[data-index="${highlighted}"]`)?.scrollIntoView({block:'nearest'});}
  function close(){open=false;search='';clearTimeout(searchTimer);}
  function choose(index:number){if(disabled||choices[index]?.disabled)return;source.selectedIndex=index;source.dispatchEvent(new Event('change',{bubbles:true}));refresh();close();trigger.focus();}
  async function move(direction:number){let next=highlighted;for(let i=0;i<choices.length;i++){next=(next+direction+choices.length)%choices.length;if(!choices[next].disabled){highlighted=next;break;}}await tick();reveal();}
  async function keydown(e:KeyboardEvent){
    e.stopPropagation();
    if(e.key==='Tab'){close();return;}
    if(e.key==='Escape'){if(open)e.preventDefault();close();return;}
    if(['ArrowDown','ArrowUp','Enter',' '].includes(e.key)){e.preventDefault();if(!open){await show();return;}if(e.key==='ArrowDown'||e.key==='ArrowUp')await move(e.key==='ArrowDown'?1:-1);else choose(highlighted);return;}
    if(e.key==='Home'||e.key==='End'){e.preventDefault();if(!open)await show();highlighted=e.key==='Home'?choices.findIndex(c=>!c.disabled):choices.findLastIndex(c=>!c.disabled);await tick();reveal();return;}
    if(e.key.length===1&&!e.ctrlKey&&!e.metaKey&&!e.altKey){e.preventDefault();if(!open)await show();clearTimeout(searchTimer);search+=e.key.toLowerCase();const next=choices.findIndex(c=>!c.disabled&&c.label.toLowerCase().startsWith(search));if(next>=0){highlighted=next;await tick();reveal();}searchTimer=setTimeout(()=>search='',600);}
  }
  function portal(node:HTMLElement){document.body.append(node);return{destroy(){node.remove();}};}
  $effect(()=>{void value;void attributes.disabled;void attributes['aria-label'];if(source)queueMicrotask(refresh);if(disabled)close();});
  onMount(()=>{
    refresh();const observer=new MutationObserver(refresh);observer.observe(source,{subtree:true,childList:true,attributes:true});
    const outside=(e:PointerEvent)=>{if(open&&!trigger.contains(e.target as Node)&&!list?.contains(e.target as Node))close();};
    const onScroll=(e:Event)=>{if(open&&!list?.contains(e.target as Node)){const r=trigger.getBoundingClientRect();if(r.bottom<=0||r.top>=innerHeight)close();else position();}};
    const focusOut=(e:FocusEvent)=>{if(open&&!trigger.contains(e.relatedTarget as Node)&&!list?.contains(e.relatedTarget as Node))close();};
    document.addEventListener('pointerdown',outside,true);document.addEventListener('scroll',onScroll,true);trigger.addEventListener('focusout',focusOut);window.addEventListener('resize',close);
    return()=>{observer.disconnect();clearTimeout(searchTimer);document.removeEventListener('pointerdown',outside,true);document.removeEventListener('scroll',onScroll,true);trigger.removeEventListener('focusout',focusOut);window.removeEventListener('resize',close);};
  });
</script>

<span class="studio-select">
  <select {...attributes} bind:this={source} bind:value tabindex="-1" aria-hidden="true" class="studio-select-source" onchange={e=>{attributes.onchange?.(e);queueMicrotask(refresh);}}>{@render children()}</select>
  <button bind:this={trigger} type="button" class={`studio-select-trigger ${attributes.class??''}`} role="combobox" aria-label={name} aria-haspopup="listbox" aria-expanded={open} aria-controls={`${uid}-list`} aria-activedescendant={open&&highlighted>=0?`${uid}-${highlighted}`:undefined} {disabled} onclick={e=>{e.preventDefault();e.stopPropagation();open?close():show();}} onkeydown={keydown}><span>{choices[selected]?.label??'Choose an option'}</span><StudioIcon name={open ? 'up' : 'down'} size={12}/></button>
</span>
{#if open}
  <div bind:this={list} use:portal id={`${uid}-list`} class="studio-select-menu" role="listbox" aria-label={name} style={`left:${left}px;top:${top}px;width:${width}px;max-height:${maxHeight}px`}>
    {#each choices as option,index}<button type="button" role="option" id={`${uid}-${index}`} aria-selected={index===selected} aria-disabled={option.disabled} disabled={option.disabled} tabindex="-1" data-index={index} class:highlighted={index===highlighted} onpointermove={()=>highlighted=index} onpointerdown={e=>e.preventDefault()} onclick={e=>{e.preventDefault();e.stopPropagation();choose(index);}}><span>{option.label}</span>{#if index===selected}<StudioIcon name="check" size={14}/>{/if}</button>{/each}
    {#if choices.length===0}<span class="select-empty">No available options</span>{/if}
  </div>
{/if}

