<template>
    <UiBox class="col-span-1 mt-4">
        <div class="adaflight_build">
            <div class="info_row">
                <strong>{{ $t("adaflightBuildCheckout") }}</strong>
                <UInput v-model="checkout" size="xs" class="w-full" @change="settingsChanged" />
            </div>
            <div class="info_row">
                <strong>{{ $t("adaflightBuildBoard") }}</strong>
                <UInput v-model="board" size="xs" @change="settingsChanged" />
                <UButton size="xs" :disabled="running" :loading="loading" @click="loadFeatures">
                    {{ $t("adaflightBuildLoadFeatures") }}
                </UButton>
            </div>
            <p v-if="info" class="board_line">
                {{ info.board }}: {{ info.mcu }}, {{ info.flash_kb }} KB,
                {{ info.crate ?? $t("adaflightBuildNoCrate") }}
            </p>
            <p v-if="info?.crate && !sizes" class="hint">{{ $t("adaflightFitNotMeasured") }}</p>
            <div v-if="info" class="features">
                <div v-for="feature in info.features" :key="feature.name" :title="feature.description">
                    <USwitch v-model="selected[feature.name]" size="sm" :label="feature.name" />
                    <span v-if="sizes && sizes.costs[feature.name] != null" class="feature_cost">
                        {{ kilobytes(sizes.costs[feature.name]!) }}
                    </span>
                    <span
                        v-if="sizes?.failures?.[feature.name]"
                        class="feature_note"
                        :class="{ feature_failure: selected[feature.name] }"
                        :title="sizes.failures[feature.name]"
                    >
                        {{ failureText(sizes.failures[feature.name]) }}
                    </span>
                </div>
            </div>
            <div v-if="fit" class="fit">
                <div class="fit_track">
                    <div class="fit_block fit_core" :style="{ width: share(fit.core) }" :title="coreTitle">
                        <span>{{ $t("adaflightFitFixed") }}</span>
                    </div>
                    <div
                        v-for="block in fit.blocks"
                        :key="block.name"
                        class="fit_block"
                        :class="{ fit_past: block.past }"
                        :style="{ width: share(block.bytes) }"
                        :title="`${block.name}: ${kilobytes(block.bytes)}`"
                    >
                        <span>{{ block.name }}</span>
                    </div>
                    <div class="fit_limit" :style="{ left: share(fit.room) }" :title="limitTitle"></div>
                </div>
                <div class="fit_legend">
                    <span><i class="swatch fit_core"></i>{{ $t("adaflightFitFixed") }} {{ kilobytes(fit.core) }}</span>
                    <span
                        ><i class="swatch fit_feature"></i>{{ $t("adaflightFitFeatures") }}
                        {{ kilobytes(fit.features) }}</span
                    >
                    <span v-if="fit.total <= fit.room">
                        <i class="swatch fit_free"></i>{{ $t("adaflightFitFree") }}
                        {{ kilobytes(fit.room - fit.total) }}
                    </span>
                    <span v-else class="fit_over">
                        <i class="swatch fit_past"></i>{{ $t("adaflightFitOver") }}
                        {{ kilobytes(fit.total - fit.room) }}
                    </span>
                    <span><i class="swatch fit_line"></i>{{ $t("adaflightFitEnd") }} {{ kilobytes(fit.room) }}</span>
                </div>
            </div>
            <div class="actions">
                <UButton
                    size="xs"
                    variant="soft"
                    :disabled="!info?.crate || running"
                    :loading="action === 'measure'"
                    @click="runMeasure"
                >
                    {{ $t("adaflightBuildMeasure") }}
                </UButton>
                <UButton size="xs" :disabled="!info?.crate || running" :loading="action === 'build'" @click="runBuild">
                    {{ $t("adaflightBuildBuild") }}
                </UButton>
                <UButton
                    size="xs"
                    color="success"
                    :disabled="!flashable || running"
                    :loading="action === 'flash'"
                    @click="runFlash"
                >
                    {{ $t("adaflightBuildFlash") }}
                </UButton>
            </div>
            <p v-if="resultText" class="result">{{ resultText }}</p>
            <p v-if="error" class="error">{{ error }}</p>
            <div class="log_toolbar">
                <USwitch v-model="autoScroll" size="sm" :label="$t('logAutoScroll')" />
                <UButton icon="i-lucide-copy" size="xs" variant="soft" color="neutral" @click="copyLog">
                    {{ copied ? $t("cliCopySuccessful") : $t("cliCopyToClipboardBtn") }}
                </UButton>
            </div>
            <pre ref="logArea" class="log">{{ log.join("\n") }}</pre>
        </div>
    </UiBox>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import UiBox from "@/components/elements/UiBox.vue";
import { get as getConfig, set as setConfig } from "@/js/ConfigStorage";
import { i18n } from "@/js/localization";
import Clipboard from "@/js/Clipboard";
import {
    build,
    changes,
    estimate,
    flash,
    loadBoard,
    loadSizes,
    measure,
    onLog,
    type AdaflightBoard,
    type AdaflightBuildReport,
    type AdaflightSizes,
} from "@/js/adaflight_build";

const MAX_LOG_LINES = 400;

const checkout = ref<string>(getConfig<string>("adaflightCheckout").adaflightCheckout ?? "");
const board = ref<string>(getConfig<string>("adaflightBoard").adaflightBoard ?? "");
const info = ref<AdaflightBoard | null>(null);
const selected = ref<Record<string, boolean>>({});
const report = ref<AdaflightBuildReport | null>(null);
const builtChanges = ref<string | null>(null);
const loading = ref(false);
const action = ref<"build" | "flash" | "measure" | null>(null);
const sizes = ref<AdaflightSizes | null>(null);
const measuredOn = ref<Record<string, boolean>>({});
const error = ref("");
const log = ref<string[]>([]);
const logLines = ref(0);
const logArea = ref<HTMLElement | null>(null);
const autoScroll = ref(true);
const copied = ref(false);
let unlisten: (() => void) | null = null;

const running = computed(() => action.value !== null);
const currentChanges = computed(() => (info.value ? changes(info.value.features, selected.value) : ""));
const flashable = computed(
    () => report.value?.ok === true && builtChanges.value === currentChanges.value && !!info.value?.crate,
);
const resultText = computed(() => {
    const r = report.value;
    if (!r) {
        return "";
    }
    if (r.ok && r.used !== null && r.room !== null) {
        return i18n.getMessage("adaflightBuildFits", [String(r.used), String(r.room), String(r.room - r.used)]);
    }
    if (r.overflow !== null) {
        return i18n.getMessage("adaflightBuildOverflow", [String(r.overflow)]);
    }
    return i18n.getMessage("adaflightBuildFailed");
});

const fit = computed(() => {
    if (!sizes.value) {
        return null;
    }
    const { total, core, blocks } = estimate(sizes.value, measuredOn.value, selected.value);
    let end = core;
    const placed = blocks.map((block) => {
        end += block.bytes;
        return { ...block, past: end > sizes.value!.room };
    });
    const features = blocks.reduce((sum, block) => sum + block.bytes, 0);
    return {
        total,
        core,
        features,
        room: sizes.value.room,
        blocks: placed,
        scale: Math.max(total, sizes.value.room) * 1.03,
    };
});
const coreTitle = computed(() => (fit.value ? i18n.getMessage("adaflightFitCore", [kilobytes(fit.value.core)]) : ""));
const limitTitle = computed(() =>
    fit.value ? `${i18n.getMessage("adaflightFitEnd")} ${kilobytes(fit.value.room)}` : "",
);

function kilobytes(bytes: number) {
    return `${(bytes / 1024).toFixed(1)} KB`;
}

function failureText(reason: string) {
    const ram = /region `(\w+)' overflowed by (\d+) bytes/.exec(reason);
    if (ram && ram[1] !== "flash") {
        return i18n.getMessage("adaflightFitRamShort", [kilobytes(Number(ram[2]))]);
    }
    return i18n.getMessage("adaflightFitNoBuild");
}

function settingsChanged() {
    saveSettings();
    if (checkout.value && board.value.trim()) {
        loadFeatures();
    }
}

function share(bytes: number) {
    return fit.value ? `max(3px, ${(100 * Math.max(bytes, 0)) / fit.value.scale}%)` : "0%";
}

async function useSizes(measured: AdaflightSizes | null) {
    sizes.value = measured;
    measuredOn.value = {};
    if (measured) {
        const at = await loadBoard(checkout.value, measured.board, measured.features);
        measuredOn.value = Object.fromEntries(at.features.map((feature) => [feature.name, feature.on]));
    }
}

function saveSettings() {
    setConfig({ adaflightCheckout: checkout.value, adaflightBoard: board.value });
}

function addLog(line: string) {
    log.value.push(line);
    logLines.value++;
    if (log.value.length > MAX_LOG_LINES) {
        log.value.splice(0, log.value.length - MAX_LOG_LINES);
    }
}

function scrollToBottom() {
    const el = logArea.value;
    if (el) {
        el.scrollTop = el.scrollHeight;
    }
}

watch(logLines, () => {
    if (autoScroll.value) {
        nextTick(scrollToBottom);
    }
});

function copyLog() {
    Clipboard.writeText(log.value.join("\n"), () => {
        copied.value = true;
        setTimeout(() => (copied.value = false), 2000);
    });
}

async function loadFeatures() {
    error.value = "";
    loading.value = true;
    try {
        saveSettings();
        info.value = await loadBoard(checkout.value, board.value.trim());
        selected.value = Object.fromEntries(info.value.features.map((feature) => [feature.name, feature.on]));
        report.value = null;
        builtChanges.value = null;
        await useSizes(info.value.crate ? await loadSizes(checkout.value, info.value.board, info.value.crate) : null);
    } catch (e) {
        info.value = null;
        error.value = String(e);
    } finally {
        loading.value = false;
    }
}

async function runBuild() {
    if (!info.value?.crate) {
        return;
    }
    error.value = "";
    log.value = [];
    action.value = "build";
    const requested = currentChanges.value;
    try {
        report.value = await build(checkout.value, info.value.board, requested, info.value.crate);
        builtChanges.value = requested;
    } catch (e) {
        error.value = String(e);
    } finally {
        action.value = null;
    }
}

async function runMeasure() {
    if (!info.value?.crate) {
        return;
    }
    error.value = "";
    log.value = [];
    action.value = "measure";
    try {
        await useSizes(await measure(checkout.value, info.value.board, currentChanges.value, info.value.crate));
    } catch (e) {
        error.value = String(e);
    } finally {
        action.value = null;
    }
}

async function runFlash() {
    if (!info.value?.crate) {
        return;
    }
    error.value = "";
    action.value = "flash";
    try {
        if (!(await flash(checkout.value, info.value.crate))) {
            error.value = i18n.getMessage("adaflightBuildFlashFailed");
        }
    } catch (e) {
        error.value = String(e);
    } finally {
        action.value = null;
    }
}

onMounted(async () => {
    unlisten = await onLog(addLog);
});

onBeforeUnmount(() => {
    unlisten?.();
});
</script>

<style scoped>
.adaflight_build {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}
.info_row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}
.features {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: 0.25rem 1rem;
}
.actions {
    display: flex;
    gap: 0.5rem;
}
.error {
    color: var(--ui-error);
}
.hint,
.feature_cost,
.feature_note {
    font-size: 0.7rem;
    color: var(--ui-text-muted);
    margin-left: 0.5rem;
}
.feature_note.feature_failure {
    color: var(--ui-error);
}
.fit_track {
    position: relative;
    display: flex;
    height: 1.75rem;
    border: 1px solid var(--ui-border);
    background: var(--ui-bg-elevated);
}
.fit_block {
    flex: none;
    overflow: hidden;
    border-right: 1px solid var(--ui-bg);
    background: var(--ui-primary);
    color: var(--ui-bg);
    font-size: 0.7rem;
    white-space: nowrap;
    padding: 0.25rem;
}
.fit_core {
    background: var(--ui-bg-accented);
    color: var(--ui-text);
}
.fit_feature {
    background: var(--ui-primary);
}
.fit_past {
    background: var(--ui-error);
}
.fit_free {
    background: var(--ui-bg-elevated);
    border: 1px solid var(--ui-border);
}
.fit_limit {
    position: absolute;
    top: -0.25rem;
    bottom: -0.25rem;
    width: 3px;
    background: var(--ui-warning);
}
.fit_line {
    background: var(--ui-warning);
}
.fit_legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 1rem;
    margin-top: 0.4rem;
    font-size: 0.75rem;
}
.fit_over {
    color: var(--ui-error);
}
.swatch {
    display: inline-block;
    width: 0.75rem;
    height: 0.75rem;
    margin-right: 0.3rem;
    vertical-align: middle;
}
.log_toolbar {
    display: flex;
    align-items: center;
    gap: 1rem;
}
.log {
    user-select: text;
    max-height: 20rem;
    overflow: auto;
    font-size: 0.75rem;
    white-space: pre-wrap;
}
</style>
