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
