import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** One feature of an adaflight build, as `tools/featuregen.py --json` lists it. */
export interface AdaflightFeature {
    name: string;
    description: string;
    default: boolean;
    on: boolean;
}

/** A board's MCU, flash size, board crate and features. */
export interface AdaflightBoard {
    board: string;
    mcu: string;
    flash_kb: number;
    crate: string | null;
    features: AdaflightFeature[];
}

/** A build's code size against its flash region, or the overflow its link or packing named. */
export interface AdaflightBuildReport {
    ok: boolean;
    used: number | null;
    room: number | null;
    overflow: number | null;
}

export async function loadBoard(checkout: string, board: string, features = ""): Promise<AdaflightBoard> {
    return JSON.parse(await invoke<string>("adaflight_features", { checkout, board, features }));
}

export function build(
    checkout: string,
    board: string,
    features: string,
    crateName: string,
): Promise<AdaflightBuildReport> {
    return invoke<AdaflightBuildReport>("adaflight_build", { checkout, board, features, crateName });
}

export function flash(checkout: string, crateName: string): Promise<boolean> {
    return invoke<boolean>("adaflight_flash", { checkout, crateName });
}

export function onLog(handler: (line: string) => void): Promise<UnlistenFn> {
    return listen<string>("adaflight-log", (event) => handler(event.payload));
}

/** The `FEATURES` list that turns a board's defaults into `selected`. */
export function changes(features: AdaflightFeature[], selected: Record<string, boolean>): string {
    return features
        .filter((feature) => selected[feature.name] !== feature.default)
        .map((feature) => (selected[feature.name] ? "+" : "-") + feature.name)
        .join(" ");
}

/** Each feature's flash cost for one board, as `tools/featuresize.py` measured it against the `features` selection. */
export interface AdaflightSizes {
    board: string;
    crate: string;
    features: string;
    used: number;
    room: number;
    costs: Record<string, number | null>;
    failures?: Record<string, string>;
}

export async function measure(
    checkout: string,
    board: string,
    features: string,
    crateName: string,
): Promise<AdaflightSizes> {
    return JSON.parse(await invoke<string>("adaflight_measure", { checkout, board, features, crateName }));
}

export async function loadSizes(checkout: string, board: string, crateName: string): Promise<AdaflightSizes | null> {
    const text = await invoke<string>("adaflight_sizes", { checkout, board, crateName });
    return text ? JSON.parse(text) : null;
}

/** A feature's block of the fit bar. */
export interface FitBlock {
    name: string;
    bytes: number;
}

/**
 * The estimated code size of `selected` from a measurement whose selection had the features of `measuredOn`: the
 * code no measured feature accounts for, then a block per selected feature, largest first.
 */
export function estimate(
    sizes: AdaflightSizes,
    measuredOn: Record<string, boolean>,
    selected: Record<string, boolean>,
): { total: number; core: number; blocks: FitBlock[] } {
    let core = sizes.used;
    let total = 0;
    const blocks: FitBlock[] = [];
    for (const [name, cost] of Object.entries(sizes.costs)) {
        if (cost === null) {
            continue;
        }
        if (measuredOn[name]) {
            core -= cost;
        }
        if (selected[name]) {
            total += cost;
            if (cost > 0) {
                blocks.push({ name, bytes: cost });
            }
        }
    }
    blocks.sort((a, b) => b.bytes - a.bytes);
    return { total: core + total, core, blocks };
}
