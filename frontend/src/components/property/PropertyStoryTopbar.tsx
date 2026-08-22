import { Link } from "react-router-dom";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";

export type PropertyStoryMode = "story" | "dossier";

type Props = {
  propertyId: string;
  title: string;
  mode: PropertyStoryMode;
  playing: boolean;
  onModeChange: (mode: PropertyStoryMode) => void;
  onPlayingChange: (playing: boolean) => void;
};

export function PropertyStoryTopbar({
  propertyId,
  title,
  mode,
  playing,
  onModeChange,
  onPlayingChange,
}: Props) {
  return (
    <header className="property-story-topbar">
      <Link className="property-story-topbar__brand" to="/" aria-label="OpenEstates home">
        <span aria-hidden="true">O</span>
        <strong>openestates</strong>
      </Link>

      <div className="property-story-topbar__modes" aria-label="Property view">
        <button
          type="button"
          className={mode === "story" ? "is-active" : ""}
          aria-pressed={mode === "story"}
          onClick={() => onModeChange("story")}
        >
          Story
        </button>
        <button
          type="button"
          className={mode === "dossier" ? "is-active" : ""}
          aria-pressed={mode === "dossier"}
          onClick={() => onModeChange("dossier")}
        >
          Full dossier
        </button>
      </div>

      <div className="property-story-topbar__actions">
        {mode === "story" && (
          <button
            type="button"
            className="property-story-topbar__play"
            aria-label={playing ? "Pause story" : "Play story"}
            aria-pressed={playing}
            onClick={() => onPlayingChange(!playing)}
          >
            <span aria-hidden="true">{playing ? "Ⅱ" : "▶"}</span>
            <strong>{playing ? "Pause story" : "Play story"}</strong>
          </button>
        )}
        <SaveHeartButton
          propertyId={propertyId}
          className="property-story-topbar__save"
          label="Save"
        />
        <NotebookCommentAnchor
          propertyId={propertyId}
          labels={[]}
          detail={title}
          source="Property detail"
          className="property-story-topbar__note"
        />
      </div>
    </header>
  );
}
