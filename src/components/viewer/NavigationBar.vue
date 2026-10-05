<script setup lang="ts">
// ============================================================
// NavigationBar - Top bar: folder, current file, mark buttons and viewer tools
// ============================================================

import { computed } from 'vue'
import { ArrowLeft, Info, Archive, Keyboard } from '@lucide/vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import ActionBar from '@/components/actions/ActionBar.vue'
import ViewerTools from './ViewerTools.vue'

const session = useSessionStore()
const view = useViewStore()

const folderName = computed(() => {
  const path = session.folderPath
  return path.split('/').pop() || path.split('\\').pop() || path
})

// Always the photo F / X act on; in compare mode each pane labels its own file
const displayPair = computed(() => session.currentPair)

const processedCount = computed(() => session.starredCount + session.deletedCount)

function goBack() {
  if (view.isOrganizing) return
  session.resetSession()
  view.setView('welcome')
}

function openArchive() {
  view.toggleArchiveDialog()
}
</script>

<template>
  <div
    class="fixed top-0 left-0 right-0 h-nav z-50 bg-[#121212]/90 backdrop-blur-md flex items-center justify-between px-4 border-b border-white/5"
  >
    <!-- Left: Back + Folder name -->
    <div class="flex items-center gap-3 shrink-0">
      <button
        class="p-1.5 rounded-lg hover:bg-white/10 transition-colors btn-spring"
        @click="goBack"
      >
        <ArrowLeft :size="16" class="text-sift-muted" />
      </button>
      <span class="text-sm text-sift-muted truncate max-w-[200px] max-lg:max-w-[120px]">
        {{ folderName }}
      </span>
    </div>

    <!-- Center: file name + RAW badge (gives way first when the window is narrow) -->
    <div v-if="displayPair" class="flex-1 min-w-0 flex items-center justify-center gap-2 px-3">
      <span class="text-sm text-white/90 font-medium truncate max-w-[300px]">
        {{
          displayPair.source === 'rawPreview' && displayPair.rawPath
            ? displayPair.rawPath.split('/').pop()?.split('\\').pop()
            : displayPair.jpgPath.split('/').pop()?.split('\\').pop()
        }}
      </span>
      <span
        v-if="displayPair.source === 'rawPreview'"
        class="px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-amber-500/20 text-amber-400"
      >
        RAW 预览
      </span>
      <span
        v-else-if="displayPair.rawFormat"
        class="px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-sift-success/20 text-sift-success"
      >
        {{ displayPair.rawFormat }}
      </span>
      <span
        v-else
        class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-sift-muted/20 text-sift-muted"
      >
        无 RAW
      </span>
    </div>

    <!-- Right: Marks | viewer tools | Archive + shortcuts + EXIF toggle -->
    <div class="flex items-center gap-2 shrink-0">
      <ActionBar />
      <div class="w-px h-4 bg-white/10" />
      <ViewerTools />
      <div class="w-px h-4 bg-white/10" />
      <button
        v-if="processedCount > 0"
        class="relative p-1.5 rounded-lg transition-colors btn-spring hover:bg-white/10 text-sift-muted hover:text-sift-accent"
        title="一键归档 (⌘Enter)"
        @click="openArchive"
      >
        <Archive :size="16" />
        <span
          class="absolute -top-1 -right-1 min-w-[16px] h-4 px-1 rounded-full bg-sift-accent text-[10px] font-bold text-white flex items-center justify-center leading-none"
        >
          {{ processedCount }}
        </span>
      </button>
      <button
        class="p-1.5 rounded-lg transition-colors btn-spring hover:bg-white/10 text-sift-muted"
        title="键盘快捷键 (?)"
        @click="view.openShortcutHelp()"
      >
        <Keyboard :size="16" />
      </button>
      <button
        class="p-1.5 rounded-lg transition-colors btn-spring"
        title="EXIF 信息 (I)"
        :class="
          view.showExifPanel
            ? 'bg-sift-accent/20 text-sift-accent'
            : 'hover:bg-white/10 text-sift-muted'
        "
        @click="view.toggleExifPanel()"
      >
        <Info :size="16" />
      </button>
    </div>
  </div>
</template>
