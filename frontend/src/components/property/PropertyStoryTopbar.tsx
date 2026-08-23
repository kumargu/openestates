import { Link } from "react-router-dom";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";

type Props = {
  propertyId: string;
  title: string;
  canPlay: boolean;
  playing: boolean;
  onPlayingChange: (playing: boolean) => void;
};

export function PropertyStoryTopbar({
  propertyId,
  title,
  canPlay,
  playing,
  onPlayingChange,
}: Props) {
  return (
    <header className="property-story-topbar">
      <Link className="property-story-topbar__brand" to="/" aria-label="OpenEstates home">
        OpenEstates
      </Link>

      <div className="property-story-topbar__actions">
        {canPlay && (
          <button
            type="button"
            className="property-story-topbar__play"
            aria-label={playing ? "Pause images" : "Play images"}
            aria-pressed={playing}
            onClick={() => onPlayingChange(!playing)}
          >
            <span aria-hidden="true">{playing ? "Ⅱ" : "▶"}</span>
            <strong>{playing ? "Pause images" : "Play images"}</strong>
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
