<script setup lang="ts">
// ============================================================
// ViewerTools - Rotate / Survey / Compare, docked in the top bar
// ============================================================

import { Columns2, LayoutGrid, RotateCw } from '@lucide/vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { useSurveyToggle } from '@/composables/useSurveyToggle'

const session = useSessionStore()
const view = useViewStore()
const { toggleSurvey } = useSurveyToggle()

const IDLE = 'hover:bg-white/10 text-sift-muted'
const ACTIVE = 'bg-sift-accent/20 text-sift-accent'
</script>

<template>
  <div class="flex items-center gap-1">
    <button
      class="p-1.5 rounded-lg transition-colors btn-spring"
      :class="IDLE"
      title="旋转 90° (R)"
      @click="view.rotateBy(90)"
    >
      <RotateCw :size="16" />
    </button>
    <button
      class="p-1.5 rounded-lg transition-colors btn-spring"
      :class="view.surveyMode ? ACTIVE : IDLE"
      :title="view.surveyMode ? '退出组内对比 (N)' : '组内并排对比 (N)'"
      @click="toggleSurvey"
    >
      <LayoutGrid :size="16" />
    </button>
    <button
      class="p-1.5 rounded-lg transition-colors btn-spring"
      :class="view.compareMode ? ACTIVE : IDLE"
      :title="view.compareMode ? '退出对比 (C)' : '对比 (C)'"
      @click="view.toggleCompare(session.currentIndex)"
    >
      <Columns2 :size="16" />
    </button>
  </div>
</template>
