import math
from typing import Literal

from pydantic import BaseModel, Field, model_validator


class Keyframe(BaseModel):
    time: float = Field(ge=0, le=86400)
    x: float = Field(default=50, ge=-100, le=200)
    y: float = Field(default=50, ge=-100, le=200)
    scale: float = Field(default=1, ge=0.05, le=4)
    opacity: float = Field(default=1, ge=0, le=1)
    rotation: float = Field(default=0, ge=-360, le=360)


class Clip(BaseModel):
    id: str = Field(min_length=1, max_length=100)
    asset_id: str | None = Field(default=None, max_length=100)
    kind: Literal["video", "audio", "image", "title", "graphic"] = "video"
    name: str = Field(default="Clip", max_length=300)
    track: int = Field(default=1, ge=0, le=4)
    start: float = Field(default=0, ge=0, le=86400)
    duration: float = Field(default=5, gt=0, le=86400)
    source_in: float = Field(default=0, ge=0, le=86400)
    speed: float = Field(default=1, gt=0)
    volume: float = Field(default=1, ge=0, le=2)
    x: float = Field(default=50, ge=-100, le=200)
    y: float = Field(default=50, ge=-100, le=200)
    scale: float = Field(default=1, ge=0.05, le=4)
    opacity: float = Field(default=1, ge=0, le=1)
    rotation: float = Field(default=0, ge=-360, le=360)
    brightness: float = Field(default=0, ge=-1, le=1)
    contrast: float = Field(default=1, ge=0, le=3)
    saturation: float = Field(default=1, ge=0, le=3)
    warmth: float = Field(default=0, ge=-1, le=1)
    blur: float = Field(default=0, ge=0, le=20)
    motion_blur: bool = False
    shutter: float = Field(default=0.5, ge=0.1, le=1)
    deflicker: bool = False
    mask: Literal["none", "circle", "rectangle"] = "none"
    transition: Literal["none", "fade", "slide", "zoom", "wipe"] = "none"
    fade_duration: float = Field(default=0.5, ge=0.05, le=5)
    text: str = Field(default="Your story starts here", max_length=3000)
    font_size: int = Field(default=64, ge=12, le=200)
    color: str = Field(default="#ffffff", pattern=r"^#[0-9a-fA-F]{6}$")
    graphic: Literal["arrow", "circle", "rectangle", "lower-third"] = "arrow"
    keyframes: list[Keyframe] = Field(default_factory=list, max_length=200)


class Project(BaseModel):
    version: Literal[1] = 1
    name: str = Field(default="Untitled project", max_length=200)
    width: int = Field(default=1920, ge=240, le=3840)
    height: int = Field(default=1080, ge=240, le=3840)
    fps: int = Field(default=30, ge=12, le=60)
    clips: list[Clip] = Field(default_factory=list, max_length=2000)
    muted_tracks: list[int] = Field(default_factory=list, max_length=6)
    hidden_tracks: list[int] = Field(default_factory=list, max_length=6)

    @model_validator(mode="after")
    def sane(self):
        if self.width % 2 or self.height % 2:
            raise ValueError("Video dimensions must be even")
        if len({c.id for c in self.clips}) != len(self.clips):
            raise ValueError("Clip IDs must be unique")
        if any(
            not math.isfinite(getattr(c, p))
            for c in self.clips
            for p in [
                "start",
                "duration",
                "speed",
                "x",
                "y",
                "scale",
                "opacity",
                "rotation",
                "volume",
                "source_in",
            ]
        ):
            raise ValueError("Clip timing must be finite")
        for c in self.clips:
            if c.kind in ("video", "audio", "image") and not c.asset_id:
                raise ValueError("Media clips require an asset ID")
            if c.duration < 1 / self.fps - 1e-8:
                raise ValueError("Each clip must last at least one project frame")
            if c.start + c.duration > 86400 + 1e-8:
                raise ValueError("Project timelines are limited to 24 hours")
            if len({k.time for k in c.keyframes}) != len(c.keyframes):
                raise ValueError("Keyframe times must be unique within each clip")
        if any(t not in range(5) for t in self.muted_tracks + self.hidden_tracks):
            raise ValueError("Track IDs must be between 0 and 4")
        return self
