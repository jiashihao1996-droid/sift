<script setup lang="ts">
// ============================================================
// ShotStrip - One photo's histogram and settings over its pane in compare
// and survey (I); settings that differ from the reference photo stand out
// ============================================================

import { computed, toRef } from 'vue'
import { ShotParam, stripHistogram, type ShotRow } from '@/core/shotCompare'
import { useHistogram } from '@/composables/useHistogram'
import HistogramPlot from './HistogramPlot.vue'

const props = defineProps<{
  jpgPath: string
  /** Settings of every photo on screen; this strip shows column `column` */
  rows: ShotRow[]
  column: number
  /** Clock time on the reference photo, the gap to it on the others */
  timeLabel: string
  isReference: boolean
  paneWidth: number
}>()

const histogramSize = computed(() => stripHistogram(props.paneWidth))

const histogramState = useHistogram(
  toRef(props, 'jpgPath'),
  computed(() => histogramSize.value !== 'none'),
)

const exposure = computed(() =>
  props.rows
    .filter((r) => r.param !== ShotParam.Lens)
    .map((r) => ({ param: r.param, label: r.label, ...r.cells[props.column] })),
)

const lens = computed(() => props.rows.find((r) => r.param === ShotParam.Lens)?.cells[props.column])
</script>

<template>
  <div
    class="flex items-stretch gap-2.5 max-w-[calc(100%-1rem)] px-2.5 py-2 rounded-lg bg-black/60 backdrop-blur-md border border-white/10 cursor-default"
    @mousedown.stop
    @click.stop
    @dblclick.stop
  >
    <HistogramPlot
      v-if="histogramSize !== 'none'"
      class="shrink-0"
      :class="histogramSize === 'full' ? 'w-28 h-11' : 'w-20 h-9'"
      :histogram="histogramState.histogram.value"
      :is-loading="histogramState.isLoading.value"
      :has-error="histogramState.hasError.value"
    />
    <div class="min-w-0 flex flex-col justify-center gap-1 font-mono text-[11px] leading-none">
      <div class="flex flex-wrap items-center gap-x-1 gap-y-1 text-white/85">
        <span
          v-for="cell in exposure"
          :key="cell.param"
          :title="cell.label"
          class="whitespace-nowrap rounded-sm px-1 py-0.5"
          :class="{ 'shot-differs': cell.differs }"
        >
          {{ cell.text || '—' }}
        </span>
      </div>
      <div class="flex items-center gap-2 px-1 text-white/45">
        <span
          v-if="lens"
          class="truncate"
          :class="{ 'shot-differs rounded-sm px-1': lens.differs }"
          title="镜头"
        >
          {{ lens.text || '未知镜头' }}
        </span>
        <span
          v-if="timeLabel"
          class="ml-auto shrink-0 tabular-nums"
          :class="isReference ? '' : 'text-white/75'"
          :title="isReference ? '拍摄时间' : '与参照图的拍摄时间差'"
        >
          {{ timeLabel }}
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Cyan, not the star's amber: a differing setting is not a pick */
.shot-differs {
  color: rgb(165 243 252);
  background: rgb(34 211 238 / 0.16);
  font-weight: 600;
}
</style>
