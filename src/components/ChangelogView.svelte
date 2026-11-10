<script lang="ts">
  import changelogSource from "../../CHANGELOG.md?raw";
  import StudioIcon from "./StudioIcon.svelte";

  interface Props {
    onBack: () => void;
  }

  let { onBack }: Props = $props();

  type Part = { text: string; bold?: boolean; code?: boolean };
  type Block =
    | { kind: "release"; version: string; status: string }
    | { kind: "group"; text: string }
    | { kind: "topic"; text: string }
    | { kind: "item"; parts: Part[] };

  /** Splits one line into plain, **bold** and `code` pieces. Nothing is ever treated as HTML. */
  function inline(line: string): Part[] {
    const parts: Part[] = [];
    const pattern = /(\*\*[^*]+\*\*|`[^`]+`)/g;
    let last = 0;
    for (const match of line.matchAll(pattern)) {
      const at = match.index ?? 0;
      if (at > last) parts.push({ text: line.slice(last, at) });
      const raw = match[0];
      if (raw.startsWith("**")) parts.push({ text: raw.slice(2, -2), bold: true });
      else parts.push({ text: raw.slice(1, -1), code: true });
      last = at + raw.length;
    }
    if (last < line.length) parts.push({ text: line.slice(last) });
    return parts;
  }

  function parse(source: string): Block[] {
    const blocks: Block[] = [];
    let started = false;
    for (const rawLine of source.split(/\r?\n/)) {
      const line = rawLine.trimEnd();
      const release = line.match(/^## \[([^\]]+)\]\s*(?:-\s*(.*))?$/);
      if (release) {
        started = true;
        blocks.push({ kind: "release", version: release[1], status: (release[2] ?? "").trim() });
        continue;
      }
      if (!started || !line.trim() || line.trim() === "---") continue;
      if (line.startsWith("### ")) blocks.push({ kind: "group", text: line.slice(4).trim() });
      else if (/^\*\*[^*]+\*\*$/.test(line.trim())) blocks.push({ kind: "topic", text: line.trim().slice(2, -2) });
      else if (line.startsWith("- ")) blocks.push({ kind: "item", parts: inline(line.slice(2).trim()) });
    }
    return blocks;
  }

  const blocks = parse(changelogSource);
  const statusLabel = (status: string) => (/development/i.test(status) ? "In development" : status);
</script>

<div class="changelog">
  <div class="changelog-bar">
    <button type="button" class="quiet-button" onclick={onBack}>
      <span class="back-icon"><StudioIcon name="chevron" size={13} /></span> Back to updates
    </button>
  </div>

  <div class="changelog-scroll" role="region" aria-label="Change log" tabindex="0">
    {#each blocks as block, i (i)}
      {#if block.kind === "release"}
        <h3 class="release">
          <span>Version {block.version}</span>
          {#if block.status}<em>{statusLabel(block.status)}</em>{/if}
        </h3>
      {:else if block.kind === "group"}
        <h4 class="group">{block.text}</h4>
      {:else if block.kind === "topic"}
        <p class="topic">{block.text}</p>
      {:else}
        <p class="item">
          {#each block.parts as part}{#if part.bold}<strong>{part.text}</strong>{:else if part.code}<code>{part.text}</code>{:else}{part.text}{/if}{/each}
        </p>
      {/if}
    {/each}
  </div>
</div>

<style>
  .changelog { display: flex; flex-direction: column; gap: 12px; min-height: 0; }
  .changelog-bar { display: flex; }
  .back-icon { display: inline-flex; transform: rotate(180deg); }
  .changelog-scroll {
    max-height: min(60vh, 560px);
    overflow-y: auto;
    padding: 4px 16px 16px;
    border-radius: 12px;
    background: var(--panel-inset);
    border: 1px solid var(--line);
  }
  .release {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 4px;
    font-size: 16px;
    letter-spacing: -0.03em;
    color: var(--text);
  }
  .release em {
    font-style: normal;
    font-size: 10px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 999px;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .group {
    margin: 18px 0 6px;
    padding-bottom: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--violet);
    border-bottom: 1px solid var(--line);
  }
  .topic { margin: 14px 0 2px; font-size: 12px; font-weight: 600; color: var(--text); }
  .item {
    position: relative;
    margin: 4px 0;
    padding-left: 16px;
    font-size: 11.5px;
    line-height: 1.6;
    color: var(--text-secondary);
  }
  .item::before {
    content: "";
    position: absolute;
    left: 3px;
    top: 0.65em;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-muted);
  }
  .item strong { color: var(--text); font-weight: 600; }
  .item code {
    font-size: 10.5px;
    padding: 1px 5px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }
  .changelog-scroll:focus-visible { outline: 2px solid var(--teal); outline-offset: 2px; }
</style>
