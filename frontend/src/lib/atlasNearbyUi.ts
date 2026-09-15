export type AtlasNearbyMode = "rest" | "browse" | "selected" | "tour";
export type AtlasNearbyFraming = "context" | "closer";
export type AtlasCameraOwner = "system" | "user";

export type AtlasNearbyUiState = Readonly<{
  mode: AtlasNearbyMode;
  categoryId: string | null;
  selectedPlaceId: string | null;
  framing: AtlasNearbyFraming;
  cameraOwner: AtlasCameraOwner;
  cameraResetVersion: number;
}>;

export type AtlasNearbyUiAction =
  | { type: "open_browse"; categoryId: string }
  | { type: "change_category"; categoryId: string }
  | { type: "select_place"; placeId: string }
  | { type: "expand_list" }
  | { type: "start_tour"; placeId: string | null }
  | { type: "show_tour_place"; categoryId: string; placeId: string | null }
  | { type: "stop_tour" }
  | { type: "set_framing"; framing: AtlasNearbyFraming }
  | { type: "user_gesture" }
  | { type: "reset_camera" }
  | { type: "close" };

export const initialAtlasNearbyUiState: AtlasNearbyUiState = {
  mode: "rest",
  categoryId: null,
  selectedPlaceId: null,
  framing: "context",
  cameraOwner: "system",
  cameraResetVersion: 0,
};

export function reduceAtlasNearbyUi(
  state: AtlasNearbyUiState,
  action: AtlasNearbyUiAction,
): AtlasNearbyUiState {
  switch (action.type) {
    case "open_browse":
      return {
        ...state,
        mode: "browse",
        categoryId: action.categoryId,
        selectedPlaceId: null,
        cameraOwner: "system",
      };
    case "change_category":
      return {
        ...state,
        mode: "browse",
        categoryId: action.categoryId,
        selectedPlaceId: null,
        cameraOwner: "system",
      };
    case "select_place":
      return {
        ...state,
        mode: "selected",
        selectedPlaceId: action.placeId,
        cameraOwner: "system",
      };
    case "expand_list":
      return {
        ...state,
        mode: "browse",
      };
    case "start_tour":
      return {
        ...state,
        mode: "tour",
        selectedPlaceId: action.placeId ?? state.selectedPlaceId,
        cameraOwner: "system",
      };
    case "show_tour_place":
      return {
        ...state,
        mode: "tour",
        categoryId: action.categoryId,
        selectedPlaceId: action.placeId ?? state.selectedPlaceId,
        cameraOwner: "system",
      };
    case "stop_tour":
      return {
        ...state,
        mode: state.selectedPlaceId ? "selected" : "browse",
      };
    case "set_framing":
      return {
        ...state,
        framing: action.framing,
        cameraOwner: "system",
      };
    case "user_gesture":
      return {
        ...state,
        mode: state.mode === "tour"
          ? state.selectedPlaceId
            ? "selected"
            : "browse"
          : state.mode,
        cameraOwner: "user",
      };
    case "reset_camera":
      return {
        ...state,
        cameraOwner: "system",
        cameraResetVersion: state.cameraResetVersion + 1,
      };
    case "close":
      return {
        ...state,
        mode: "rest",
        cameraOwner: "system",
      };
  }
}
