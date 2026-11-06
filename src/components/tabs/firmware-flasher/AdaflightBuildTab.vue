<template>
    <UiBox class="col-span-1 mt-4">
        <div class="adaflight_build">
            <div class="info_row">
                <strong>{{ $t("adaflightBuildCheckout") }}</strong>
                <UInput v-model="checkout" size="xs" class="w-full" @change="saveSettings" />
            </div>
            <div class="info_row">
                <strong>{{ $t("adaflightBuildBoard") }}</strong>
                <UInput v-model="board" size="xs" @change="saveSettings" />
                <UButton size="xs" :disabled="running" :loading="loading" @click="loadFeatures">
                    {{ $t("adaflightBuildLoadFeatures") }}
                </UButton>
            </div>
            <p v-if="info" class="board_line">
                {{ info.board }}: {{ info.mcu }}, {{ info.flash_kb }} KB,
                {{ info.crate ?? $t("adaflightBuildNoCrate") }}
            </p>
            <div v-if="info" class="features">
                <div v-for="feature in info.features" :key="feature.name" :title="feature.description">
                    <USwitch v-model="selected[feature.name]" size="sm" :label="feature.name" />
                </div>
            </div>
            <div class="actions">
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
            <pre class="log">{{ log.join("\n") }}</pre>
        </div>
    </UiBox>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import UiBox from "@/components/elements/UiBox.vue";
import { get as getConfig, set as setConfig } from "@/js/ConfigStorage";
import { i18n } from "@/js/localization";
import {
    build,
    changes,
    flash,
    loadBoard,
    onLog,
    type AdaflightBoard,
    type AdaflightBuildReport,
} from "@/js/adaflight_build";

const MAX_LOG_LINES = 400;

const checkout = ref<string>(getConfig<string>("adaflightCheckout").adaflightCheckout ?? "");
const board = ref<string>(getConfig<string>("adaflightBoard").adaflightBoard ?? "");
const info = ref<AdaflightBoard | null>(null);
const selected = ref<Record<string, boolean>>({});
const report = ref<AdaflightBuildReport | null>(null);
const builtChanges = ref<string | null>(null);
const loading = ref(false);
const action = ref<"build" | "flash" | null>(null);
const error = ref("");
const log = ref<string[]>([]);
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

function saveSettings() {
    setConfig({ adaflightCheckout: checkout.value, adaflightBoard: board.value });
}

function addLog(line: string) {
    log.value.push(line);
    if (log.value.length > MAX_LOG_LINES) {
        log.value.splice(0, log.value.length - MAX_LOG_LINES);
    }
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
.log {
    max-height: 20rem;
    overflow: auto;
    font-size: 0.75rem;
    white-space: pre-wrap;
}
</style>
