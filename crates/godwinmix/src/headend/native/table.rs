//! The headend's own setup as rows of the method table. The station
//! answers every one (`calls.rs`); a core with no station in front of it
//! keeps no channels and says so.

use crate::control::call::Call;
use crate::control::methods::handler;
use godwinmix_protocol::headend::{
    ChannelAddRequest, ChannelBulkRequest, ChannelChange, ChannelNames, ChannelUpdateRequest, HeadendExport, HeadendSetup, ProfileChange, ProfileRemoveRequest, ProfileSaveRequest, ScanRequest, ScanResult,
};
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry, SchemaFn};
use godwinmix_protocol::scope::Scope;

fn row(name: &'static str, scope: Scope, summary: &'static str, params: SchemaFn, result: SchemaFn) -> MethodDef<Call> {
    MethodDef::new(name, scope, summary, handler(move |_call: Call, _| async move { Err::<serde_json::Value, _>(super::super::methods::alone(name)) })).params(params).result(result)
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(row(
        "headend.channel.list",
        Scope::Read,
        "Every headend channel: the ones the station keeps (set up here, or imported), then the \
         streams of the Flussonic config it follows, each with its input, profile, HLS settings, \
         outputs and the show it runs as. Also every transcode profile, built in and saved, and \
         the video encoders this machine has.",
        no_params,
        schema_of::<HeadendSetup>,
    ));
    reg.register(
        row(
            "headend.channel.add",
            Scope::Admin,
            "Add channels: each a direct show with its input (UDP or RTP multicast or unicast \
             MPEG-TS with a program, SRT, RTMP, RTSP, HLS or a file, and a backup), a profile \
             (copy, a built in or saved one, or custom), its HLS output and any pushes. Checked \
             and priced by show.add_many's dry run first; with dry_run nothing is made. Kept by \
             the station across restarts.",
            schema_of::<ChannelAddRequest>,
            schema_of::<ChannelChange>,
        )
        .not_idempotent(),
    );
    reg.register(row(
        "headend.channel.update",
        Scope::Admin,
        "Replace one channel whole. Only a change of input, profile or outputs makes its show \
         again; a title or enabled moves in place. A stream of the followed Flussonic config is \
         taken over: the station keeps it from then on and the file's later edits to it are not \
         applied until headend.channel.follow.",
        schema_of::<ChannelUpdateRequest>,
        schema_of::<ChannelChange>,
    ));
    reg.register(row(
        "headend.channel.bulk",
        Scope::Admin,
        "The same change to many channels at once: a profile (or custom settings), enabled, or \
         the network card every multicast input joins on.",
        schema_of::<ChannelBulkRequest>,
        schema_of::<ChannelChange>,
    ));
    reg.register(
        row(
            "headend.channel.remove",
            Scope::Admin,
            "Remove channels and their shows. A stream of the followed Flussonic config is hidden \
             from it, not deleted from the file; headend.channel.follow brings it back.",
            schema_of::<ChannelNames>,
            schema_of::<ChannelChange>,
        )
        .destructive(),
    );
    reg.register(row(
        "headend.channel.follow",
        Scope::Admin,
        "Give channels taken over from the followed Flussonic config back to it: the station \
         stops keeping them and runs them as the file says.",
        schema_of::<ChannelNames>,
        schema_of::<ChannelChange>,
    ));
    reg.register(row(
        "headend.profile.save",
        Scope::Admin,
        "Save a transcode profile (codec, size and fit, bitrate, keyframes, deinterlace, the \
         encoder on the GPU or the CPU, sound codec, bitrate and tracks), new or changed. Every \
         channel that takes it follows the change.",
        schema_of::<ProfileSaveRequest>,
        schema_of::<ProfileChange>,
    ));
    reg.register(
        row(
            "headend.profile.remove",
            Scope::Admin,
            "Remove a saved profile. One some channel takes is refused unless move_to names the \
             profile those channels take instead.",
            schema_of::<ProfileRemoveRequest>,
            schema_of::<ProfileChange>,
        )
        .destructive(),
    );
    reg.register(
        row(
            "headend.scan",
            Scope::Admin,
            "Listen to a multiplex (a multicast group and port, and the card) for a few seconds \
             and list its programs: number, name and provider from the SDT, the PMT's streams, \
             and the picture size, frame rate, sound codecs, channels and languages each one \
             sent. Then add the ones wanted with headend.channel.add.",
            schema_of::<ScanRequest>,
            schema_of::<ScanResult>,
        )
        .mutating(false),
    );
    reg.register(row(
        "headend.export",
        Scope::Read,
        "The channels the station keeps, written in Flussonic's config language, with a note \
         for each setting Flussonic has no word for.",
        no_params,
        schema_of::<HeadendExport>,
    ));
}
