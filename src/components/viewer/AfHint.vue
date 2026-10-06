<script setup lang="ts">
// ============================================================
// AfHint - Why a photo shows no focus marks while the overlay is on
// ============================================================

import { computed } from 'vue'
import { afHint } from '@/core/afPoint'
import type { AfInfo } from '@/types'

const props = defineProps<{ info?: AfInfo }>()

const hint = computed(() => afHint(props.info))
</script>

<template>
  <span
    v-if="hint"
    class="px-2.5 py-1 rounded-lg bg-black/50 backdrop-blur-sm text-[11px] text-white/70 pointer-events-none whitespace-nowrap"
  >
    {{ hint }}
  </span>
</template>
