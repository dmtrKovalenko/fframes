//! Owned caption data. All timestamps are seconds on the same clock; ranges are `[start, end)`.

use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    ops::{Deref, DerefMut},
    sync::{Arc, OnceLock},
};

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

/// Application-defined JSON data retained by subtitle loaders and callbacks.
///
/// The document is parsed once. [`typed`](Self::typed) lazily decodes the retained
/// object as the requested Serde type, without registration. Each type's successful
/// value or failure is cached and shared across frames, cloned tracks and threads.
/// Raw map access (`get`, indexing, `insert`, etc.) remains available.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptionMetadata {
    raw: Arc<Map<String, Value>>,
    #[serde(skip)]
    parsed: Arc<OnceLock<TypedMetadata>>,
}

type MetadataValue = Box<dyn Any + Send + Sync>;

// An append-only type cache keeps references stable without leaking values or
// returning a reference through a lock guard. Most metadata uses one or two types.
// OnceLock also serializes concurrent first access, including failed decodes.
struct TypedMetadata {
    type_id: TypeId,
    value: OnceLock<Option<MetadataValue>>,
    next: OnceLock<Box<TypedMetadata>>,
}

impl TypedMetadata {
    fn new<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            value: OnceLock::new(),
            next: OnceLock::new(),
        }
    }
}

impl CaptionMetadata {
    /// An empty metadata object, also used for captions without metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode `T` on first use, then borrow its cached result. No registration is needed.
    ///
    /// Returns `None` for empty or incompatible metadata. Successes and failures
    /// are cached independently for every requested type. Clones share the cache,
    /// including before first access. Mutating the raw object detaches that clone's
    /// cache so its next access decodes the edited data. The JSON string is never
    /// reparsed, and unrequested application types are never decoded.
    ///
    /// ```
    /// use fframes_media::CaptionTrack;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct Highlight {
    ///     effect: String,
    ///     strength: f32,
    /// }
    ///
    /// let track = CaptionTrack::parse(r#"{"cues":[{"start":0,"end":1,
    ///     "words":[{"text":"Moon","start":0,"end":1,
    ///         "metadata":{"effect":"quartz","strength":0.8}}]}]}"#)?;
    /// let highlight = track.cues()[0].words[0].metadata.typed::<Highlight>();
    /// assert_eq!(highlight.unwrap().effect, "quartz");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn typed<T: DeserializeOwned + Send + Sync + 'static>(&self) -> Option<&T> {
        let mut cached = self.parsed.get_or_init(TypedMetadata::new::<T>);
        while cached.type_id != TypeId::of::<T>() {
            cached = cached
                .next
                .get_or_init(|| Box::new(TypedMetadata::new::<T>()));
        }
        cached
            .value
            .get_or_init(|| {
                self.parse::<T>()
                    .map(|value| Box::new(value) as MetadataValue)
            })
            .as_deref()?
            .downcast_ref()
    }

    /// Decode metadata once for caller-managed preparation, without caching it.
    ///
    /// Empty metadata or any deserialization error returns `None`, without
    /// changing the source data or failing subtitle loading. Serde attributes
    /// on `T` control defaults and unknown-field handling. Deserialization borrows
    /// the JSON values, so `T` may contain borrowed strings and no raw map clone
    /// or intermediate JSON string is needed.
    ///
    /// This low-level helper invokes Serde on every call. Prefer
    /// [`typed`](Self::typed) during rendering. Use
    /// this helper for explicit preparation that needs borrowed values or a
    /// caller-managed lifetime.
    ///
    /// ```
    /// use fframes_media::{CaptionMetadata, Subtitles};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Highlight<'a> {
    ///     effect: &'a str,
    ///     strength: f32,
    /// }
    ///
    /// let track = Subtitles::parse(r#"{"cues":[{"start":0,"end":1,"text":"Moon",
    ///     "metadata":{"effect":"quartz","strength":0.8}}]}"#)?;
    /// let settings = track.cues()[0].metadata.parse::<Highlight<'_>>();
    /// assert_eq!(settings.unwrap().effect, "quartz");
    /// assert!(CaptionMetadata::new().parse::<Highlight<'_>>().is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse<'de, T>(&'de self) -> Option<T>
    where
        T: Deserialize<'de>,
    {
        if self.raw.is_empty() {
            return None;
        }
        T::deserialize(MetadataDeserializer(&self.raw)).ok()
    }

    /// Recover the raw JSON object, cloning only when another consumer shares it.
    pub fn into_inner(self) -> Map<String, Value> {
        Arc::unwrap_or_clone(self.raw)
    }
}

impl fmt::Debug for CaptionMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CaptionMetadata").field(&self.raw).finish()
    }
}

impl PartialEq for CaptionMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl From<Map<String, Value>> for CaptionMetadata {
    fn from(metadata: Map<String, Value>) -> Self {
        Self {
            raw: Arc::new(metadata),
            parsed: Arc::default(),
        }
    }
}

impl Deref for CaptionMetadata {
    type Target = Map<String, Value>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl DerefMut for CaptionMetadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parsed = Arc::default();
        Arc::make_mut(&mut self.raw)
    }
}

// serde_json's borrowed Map deserializer handles objects and enums, but forwards
// Option and newtype structs to visit_map. Supply the same wrapper semantics as
// deserializing a Value::Object, without cloning the map into a temporary Value.
struct MetadataDeserializer<'de>(&'de Map<String, Value>);

impl<'de> Deserializer<'de> for MetadataDeserializer<'de> {
    type Error = serde_json::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.0.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.0.deserialize_enum(name, variants, visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

/// A caption input or preparation error, including the offending input field where possible.
#[derive(Debug)]
pub enum CaptionError {
    /// Data or rendering options that cannot produce a valid caption.
    Invalid { path: String, message: String },
    /// Malformed JSON, an unknown field, or a value with the wrong type.
    Json(serde_json::Error),
    /// A `WebVTT` parsing error.
    Vtt(String),
}

impl CaptionError {
    /// Construct an error for a particular input field, such as `cues[0].words[1].end`.
    pub fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for CaptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { path, message } => write!(f, "{path}: {message}"),
            Self::Json(error) => write!(f, "caption JSON: {error}"),
            Self::Vtt(error) => write!(f, "caption WebVTT: {error}"),
        }
    }
}

impl Error for CaptionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for CaptionError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// A word's timing state. An active word occupies `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordState {
    Upcoming,
    Active,
    Spoken,
}

fn space_before_default() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// One timed display token. Usually a word, but it may be punctuation or a CJK segment.
///
/// `space_before` defaults to true. Set it to false for punctuation or adjacent tokens;
/// the renderer always ignores it for the first token of a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default = "space_before_default", skip_serializing_if = "is_true")]
    pub space_before: bool,
    /// Application-defined data, such as authored layout, motion or highlight settings.
    /// The subtitle parser preserves this object without interpreting its keys.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: CaptionMetadata,
}

impl CaptionWord {
    /// Declare a word with timestamps in seconds. The track validates its timing.
    pub fn new(text: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            text: text.into(),
            start,
            end,
            id: None,
            space_before: true,
            metadata: CaptionMetadata::new(),
        }
    }

    /// Give the word a stable identifier within its cue.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Control whether a separator is inserted before this token.
    pub fn space_before(mut self, space_before: bool) -> Self {
        self.space_before = space_before;
        self
    }

    /// Attach renderer-specific data without changing this word's text or timing.
    pub fn with_metadata(mut self, metadata: impl Into<CaptionMetadata>) -> Self {
        self.metadata = metadata.into();
        self
    }

    /// The timing state at a time in seconds. NaN has no active word.
    pub fn state_at(&self, seconds: f64) -> WordState {
        if seconds.is_nan() || seconds < self.start {
            WordState::Upcoming
        } else if seconds >= self.end {
            WordState::Spoken
        } else {
            WordState::Active
        }
    }

    /// Elapsed fraction of this word, clamped to `0..=1`; NaN returns zero.
    pub fn progress_at(&self, seconds: f64) -> f32 {
        if seconds.is_nan() {
            0.0
        } else {
            ((seconds - self.start) / (self.end - self.start)).clamp(0.0, 1.0) as f32
        }
    }
}

/// A display interval containing either plain `text` or explicitly timed `words`.
///
/// The interval may start before its first word or end after its last word, so a whole
/// phrase can remain on screen during pauses. Word timestamps use the track clock rather
/// than time relative to this cue. Overlapping cues can represent different speakers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionCue {
    pub start: f64,
    pub end: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<CaptionWord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// Application-defined cue data. Interpretation and validation belong to the renderer.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: CaptionMetadata,
    /// The original `WebVTT` identifier. It is not required to be unique.
    #[serde(skip)]
    pub name: Option<String>,
    /// Raw `WebVTT` cue note, when present.
    #[serde(skip)]
    pub note: Option<String>,
    /// Parsed positioning settings, retained for a renderer to interpret.
    #[serde(skip)]
    pub cue_settings: Option<webvtt_parser::VttCueSettings>,
    #[serde(skip)]
    resolved_text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CueInput {
    start: f64,
    end: f64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<CaptionWord>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    metadata: CaptionMetadata,
}

impl<'de> Deserialize<'de> for CaptionCue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = CueInput::deserialize(deserializer)?;
        let mut cue = Self {
            start: input.start,
            end: input.end,
            text: input.text,
            words: input.words,
            id: input.id,
            speaker: input.speaker,
            metadata: input.metadata,
            name: None,
            note: None,
            cue_settings: None,
            resolved_text: String::new(),
        };
        cue.resolved_text = cue.display_text();
        Ok(cue)
    }
}

impl CaptionCue {
    /// A phrase with explicitly timed words, displayed over `[start, end)` seconds.
    pub fn words(start: f64, end: f64, words: impl IntoIterator<Item = CaptionWord>) -> Self {
        let mut cue = Self {
            start,
            end,
            text: String::new(),
            words: words.into_iter().collect(),
            id: None,
            speaker: None,
            metadata: CaptionMetadata::new(),
            name: None,
            note: None,
            cue_settings: None,
            resolved_text: String::new(),
        };
        cue.resolved_text = cue.display_text();
        cue
    }

    /// A phrase without word timings. No per-word timestamps are invented.
    pub fn text(start: f64, end: f64, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            start,
            end,
            resolved_text: text.clone(),
            text,
            words: Vec::new(),
            id: None,
            speaker: None,
            metadata: CaptionMetadata::new(),
            name: None,
            note: None,
            cue_settings: None,
        }
    }

    /// Give the cue a stable identifier within its track.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Label the speaker without inferring a rendering style or voice identity.
    pub fn speaker(mut self, speaker: impl Into<String>) -> Self {
        self.speaker = Some(speaker.into());
        self
    }

    /// Attach renderer-specific data without changing this cue's text or timing.
    pub fn with_metadata(mut self, metadata: impl Into<CaptionMetadata>) -> Self {
        self.metadata = metadata.into();
        self
    }

    /// Reconstruct the phrase without losing the declared token separators.
    pub fn display_text(&self) -> String {
        if self.words.is_empty() {
            return self.text.clone();
        }
        let mut result = String::new();
        for (index, word) in self.words.iter().enumerate() {
            if index > 0 && word.space_before {
                result.push(' ');
            }
            result.push_str(&word.text);
        }
        result
    }

    /// The complete phrase, cached when the containing subtitle track is validated.
    /// Timed-word cues preserve their explicit token separators.
    pub fn phrase(&self) -> &str {
        if self.words.is_empty() {
            &self.text
        } else {
            &self.resolved_text
        }
    }
}

/// A validated, owned caption track suitable for Rust declarations or runtime JSON.
///
/// Cues are ordered by start time and may overlap. Words within a cue are ordered and do
/// not overlap. IDs are optional; cue IDs are unique within the track and word IDs within
/// each cue. All times are finite, nonnegative seconds with end-exclusive intervals.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptionTrack {
    cues: Arc<[CaptionCue]>,
    #[serde(skip)]
    identity: Arc<()>,
    /// Top-level `WebVTT` metadata, preserved during parsing.
    #[serde(skip)]
    pub slugs: HashMap<String, String>,
    /// Raw global `WebVTT` styling, when exposed by the parser.
    #[serde(skip)]
    pub style: Option<String>,
    #[serde(skip)]
    source_vtt: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackInput {
    cues: Vec<CaptionCue>,
}

// Dispatch top-level JSON without an intermediate Value tree. Feeding `cues`
// directly into the ordinary cue/word deserializers preserves duplicate-field
// errors that would otherwise be lost when collecting JSON keys into a map.
struct SubtitleDocumentInput {
    cues: Option<Vec<CaptionCue>>,
}

impl<'de> Deserialize<'de> for SubtitleDocumentInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DocumentVisitor;

        impl<'de> serde::de::Visitor<'de> for DocumentVisitor {
            type Value = SubtitleDocumentInput;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object with optional subtitle cues")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut cues = None;
                let mut unknown_field = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "cues" {
                        if cues.is_some() {
                            return Err(serde::de::Error::duplicate_field("cues"));
                        }
                        cues = Some(map.next_value::<Vec<CaptionCue>>()?);
                    } else {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        unknown_field.get_or_insert(key);
                    }
                }
                if cues.is_some()
                    && let Some(field) = unknown_field
                {
                    return Err(serde::de::Error::unknown_field(&field, &["cues"]));
                }
                Ok(SubtitleDocumentInput { cues })
            }
        }

        deserializer.deserialize_map(DocumentVisitor)
    }
}

impl<'de> Deserialize<'de> for CaptionTrack {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = TrackInput::deserialize(deserializer)?;
        Self::new(input.cues).map_err(serde::de::Error::custom)
    }
}

impl CaptionTrack {
    /// Validate and own a sequence of cues. An empty track is valid.
    pub fn new(cues: impl IntoIterator<Item = CaptionCue>) -> Result<Self, CaptionError> {
        let mut track = Self {
            cues: cues.into_iter().collect::<Vec<_>>().into(),
            identity: Arc::new(()),
            slugs: HashMap::new(),
            style: None,
            source_vtt: false,
        };
        track.validate()?;
        for cue in Arc::make_mut(&mut track.cues) {
            cue.resolved_text = cue.display_text();
        }
        Ok(track)
    }

    /// Parse ordinary JSON or `WebVTT`. The format is detected from its contents,
    /// independent of the filename. All parsed data is owned.
    pub fn parse(content: &str) -> Result<Self, CaptionError> {
        let content = normalize_input(content);
        if content.starts_with("WEBVTT") {
            Self::from_vtt(content)
        } else if content.starts_with('{') {
            Self::from_json(content)
        } else {
            Err(CaptionError::invalid(
                "format",
                "expected a WEBVTT document or a JSON object with a cues array",
            ))
        }
    }

    /// Classify and load a JSON document with a single textual JSON parse.
    ///
    /// Returns a validated subtitle track when the top-level object declares
    /// `cues`, or `None` for other valid JSON documents. Malformed JSON and invalid
    /// subtitle data return an error. Media loaders can retain this result instead
    /// of parsing the document once for format detection and again for loading.
    pub fn parse_json_document(content: &str) -> Result<Option<Self>, CaptionError> {
        let content = normalize_input(content);
        if !content.starts_with('{') {
            // Non-object JSON is not subtitles, but must still be valid JSON.
            serde_json::from_str::<serde::de::IgnoredAny>(content)?;
            return Ok(None);
        }
        let input: SubtitleDocumentInput = serde_json::from_str(content)?;
        input.cues.map(Self::new).transpose()
    }

    /// Whether a valid JSON object declares subtitle cues. Cue validation is left
    /// to `parse`, so malformed cue values still produce subtitle-specific errors.
    /// This is an inspection helper. Loaders should use [`Self::parse_json_document`]
    /// and retain its result to avoid parsing the document again.
    pub fn is_json(content: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(normalize_input(content)).is_ok_and(|value| {
            value
                .as_object()
                .is_some_and(|object| object.contains_key("cues"))
        })
    }

    /// Equivalent to [`Self::is_json`]. Loaders should prefer [`Self::parse_json_document`].
    pub fn is_json_subtitles(content: &str) -> bool {
        Self::is_json(content)
    }

    /// Parse the canonical JSON representation and validate all cue and word timings.
    pub fn from_json(json: &str) -> Result<Self, CaptionError> {
        let input: TrackInput = serde_json::from_str(normalize_input(json))?;
        Self::new(input.cues)
    }

    /// Serialize the canonical JSON representation with indented formatting.
    pub fn to_json_pretty(&self) -> Result<String, CaptionError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Import `WebVTT` through the existing parser, preserving raw payload and metadata.
    /// Word timestamps are never inferred from phrase text or inline markup.
    pub fn from_vtt(vtt: &str) -> Result<Self, CaptionError> {
        let parsed = webvtt_parser::Vtt::parse(normalize_input(vtt))
            .map_err(|error| CaptionError::Vtt(error.to_string()))?;
        let slugs = parsed
            .slugs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        let style = parsed.style.map(str::to_owned);
        let mut cues = Vec::with_capacity(parsed.cues.len());
        for cue in parsed.cues {
            // WebVTT permits spacer cues; they have no visible content.
            if cue.text.trim().is_empty() {
                continue;
            }
            let mut imported = CaptionCue::text(
                cue.start.as_milliseconds() as f64 / 1000.0,
                cue.end.as_milliseconds() as f64 / 1000.0,
                cue.text,
            );
            imported.name = cue.name.map(str::to_owned);
            imported.note = cue.note.map(str::to_owned);
            imported.cue_settings = cue.cue_settings;
            cues.push(imported);
        }
        let mut track = Self::new(cues)?;
        track.slugs = slugs;
        track.style = style;
        track.source_vtt = true;
        Ok(track)
    }

    /// Whether this track was parsed from `WebVTT`. Raw payload markup/settings
    /// remain available for a renderer to interpret explicitly.
    pub fn is_vtt(&self) -> bool {
        self.source_vtt
    }

    /// The ordered, validated cues. Modify through a new track so validation stays intact.
    pub fn cues(&self) -> &[CaptionCue] {
        &self.cues
    }

    /// Clones share this token, while a newly loaded revision receives a new identity.
    #[doc(hidden)]
    pub fn cache_identity(&self) -> &Arc<()> {
        &self.identity
    }

    /// All visible cues at `seconds`, including simultaneously active speakers.
    pub fn active_cues(&self, seconds: f64) -> impl Iterator<Item = (usize, &CaptionCue)> {
        self.cues
            .iter()
            .enumerate()
            .take_while(move |(_, cue)| cue.start <= seconds)
            .filter(move |(_, cue)| seconds < cue.end)
    }

    /// Validate timing, content, ordering and optional identifiers.
    pub fn validate(&self) -> Result<(), CaptionError> {
        let mut cue_ids = HashSet::new();
        let mut previous_start = 0.0;
        for (cue_index, cue) in self.cues.iter().enumerate() {
            let path = format!("cues[{cue_index}]");
            validate_interval(cue.start, cue.end, &path)?;
            if cue.start < previous_start {
                return Err(CaptionError::invalid(
                    format!("{path}.start"),
                    "cues must be ordered by start time",
                ));
            }
            previous_start = cue.start;
            validate_id(cue.id.as_deref(), &mut cue_ids, &format!("{path}.id"))?;
            if cue
                .speaker
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(CaptionError::invalid(
                    format!("{path}.speaker"),
                    "speaker must not be blank",
                ));
            }
            if cue.words.is_empty() {
                if cue.text.trim().is_empty() {
                    return Err(CaptionError::invalid(
                        &path,
                        "provide nonempty text or a nonempty words array",
                    ));
                }
                continue;
            }
            if !cue.text.is_empty() {
                return Err(CaptionError::invalid(
                    &path,
                    "provide either text or words, not both",
                ));
            }
            let mut word_ids = HashSet::new();
            let mut previous_end = cue.start;
            for (word_index, word) in cue.words.iter().enumerate() {
                let word_path = format!("{path}.words[{word_index}]");
                validate_interval(word.start, word.end, &word_path)?;
                if word.start < cue.start || word.end > cue.end {
                    return Err(CaptionError::invalid(
                        &word_path,
                        "word interval must be contained in its cue interval",
                    ));
                }
                if word.start < previous_end {
                    return Err(CaptionError::invalid(
                        format!("{word_path}.start"),
                        "words must be ordered and must not overlap",
                    ));
                }
                if word.text.trim().is_empty() {
                    return Err(CaptionError::invalid(
                        format!("{word_path}.text"),
                        "word text must not be blank",
                    ));
                }
                validate_id(
                    word.id.as_deref(),
                    &mut word_ids,
                    &format!("{word_path}.id"),
                )?;
                previous_end = word.end;
            }
        }
        Ok(())
    }
}

fn normalize_input(content: &str) -> &str {
    content
        .trim_start()
        .strip_prefix('\u{feff}')
        .unwrap_or(content.trim_start())
        .trim_start()
}

fn validate_interval(start: f64, end: f64, path: &str) -> Result<(), CaptionError> {
    for (name, value) in [("start", start), ("end", end)] {
        if !value.is_finite() || value < 0.0 {
            return Err(CaptionError::invalid(
                format!("{path}.{name}"),
                "time must be a finite, nonnegative number of seconds",
            ));
        }
    }
    if end <= start {
        return Err(CaptionError::invalid(
            format!("{path}.end"),
            "end must be greater than start (intervals are end-exclusive)",
        ));
    }
    Ok(())
}

fn validate_id<'a>(
    id: Option<&'a str>,
    seen: &mut HashSet<&'a str>,
    path: &str,
) -> Result<(), CaptionError> {
    if let Some(id) = id {
        if id.trim().is_empty() {
            return Err(CaptionError::invalid(path, "id must not be blank"));
        }
        if !seen.insert(id) {
            return Err(CaptionError::invalid(
                path,
                "id must be unique in its scope",
            ));
        }
    }
    Ok(())
}
