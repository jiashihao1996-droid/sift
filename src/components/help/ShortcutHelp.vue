<script setup lang="ts">
// ============================================================
// ShortcutHelp - Keyboard shortcut reference (toggled with ?)
// ============================================================

import { X } from '@lucide/vue'
import { useViewStore } from '@/stores/viewStore'
import { SHORTCUT_GROUPS } from './shortcuts'

const view = useViewStore()
</script>

<template>
  <Transition name="backdrop">
    <div
      v-if="view.showShortcutHelp"
      class="fixed inset-0 z-50 flex items-center justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="shortcut-help-title"
    >
      <div class="absolute inset-0 bg-black/60 backdrop-blur-xs" @click="view.closeShortcutHelp" />

      <div
        class="relative w-full max-w-2xl mx-4 p-6 rounded-2xl bg-sift-surface border border-white/5 shadow-2xl"
      >
        <div class="flex items-center justify-between mb-5">
          <h3 id="shortcut-help-title" class="text-base font-semibold text-white">键盘快捷键</h3>
          <button
            class="p-1.5 rounded-lg hover:bg-white/10 transition-colors btn-spring"
            title="关闭 (Esc)"
            @click="view.closeShortcutHelp"
          >
            <X :size="16" class="text-sift-muted" />
          </button>
        </div>

        <div class="sm:columns-2 gap-x-8">
          <section
            v-for="group in SHORTCUT_GROUPS"
            :key="group.title"
            class="break-inside-avoid pb-5 last:pb-0"
          >
            <h4 class="text-[11px] font-medium text-sift-muted uppercase tracking-wider mb-2">
              {{ group.title }}
            </h4>
            <dl class="space-y-1.5">
              <div
                v-for="item in group.items"
                :key="item.label"
                class="flex items-center justify-between gap-4"
              >
                <dt class="text-xs text-white/70">{{ item.label }}</dt>
                <dd class="flex items-center gap-1 shrink-0">
                  <template v-for="(chord, i) in item.keys" :key="i">
                    <span v-if="i > 0" class="text-[10px] text-white/25">/</span>
                    <kbd
                      v-for="key in chord"
                      :key="key"
                      class="min-w-[20px] px-1.5 py-0.5 rounded-sm bg-white/6 border border-white/10 text-[10px] text-white/60 text-center font-sans"
                    >
                      {{ key }}
                    </kbd>
                  </template>
                </dd>
              </div>
            </dl>
          </section>
        </div>
      </div>
    </div>
  </Transition>
</template>
