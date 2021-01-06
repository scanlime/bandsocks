use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str = "jrottenberg/ffmpeg:3-scratch@sha256:3396ea2f9b2224de47275cabf8ac85ee765927f6ebdc9d044bb22b7c104fedbd";

async fn common() -> ContainerBuilder {
    let _ = env_logger::builder().is_test(true).try_init();
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn pull() {
    Runtime::new().unwrap().block_on(async {
        common().await;
    })
}

#[test]
fn ffmpeg_ldso() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .entrypoint(&["/lib/ld-musl-x86_64.so.1"])
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr_str(),
            concat!(
                "musl libc (x86_64)\n",
                "Version 1.1.19\n",
                "Dynamic Program Loader\n",
                "Usage: /lib/ld-musl-x86_64.so.1 [options] [--] pathname [args]\n",
            )
        );
    })
}

#[test]
fn ffmpeg_help() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.spawn().unwrap();
        let output = container.output().await.unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stderr_str(), concat!(
            "ffmpeg version 3.2.15 Copyright (c) 2000-2020 the FFmpeg developers\n",
            "  built with gcc 6.4.0 (Alpine 6.4.0)\n",
            "  configuration: --disable-debug --disable-doc --disable-ffplay --enable-shared --enable-avresample --enable-libopencore-amrnb --enable-libopencore-amrwb --enable-gpl --enable-libass --enable-fontconfig --enable-libfreetype --enable-libvidstab --enable-libmp3lame --enable-libopus --enable-libtheora --enable-libvorbis --enable-libvpx --enable-libwebp --enable-libxcb --enable-libx265 --enable-libxvid --enable-libx264 --enable-nonfree --enable-openssl --enable-libfdk_aac --enable-postproc --enable-small --enable-version3 --enable-libbluray --enable-libzmq --extra-libs=-ldl --prefix=/opt/ffmpeg --enable-libopenjpeg --enable-libkvazaar --extra-cflags=-I/opt/ffmpeg/include --extra-ldflags=-L/opt/ffmpeg/lib\n",
            "  libavutil      55. 34.101 / 55. 34.101\n",
            "  libavcodec     57. 64.101 / 57. 64.101\n",
            "  libavformat    57. 56.101 / 57. 56.101\n",
            "  libavdevice    57.  1.100 / 57.  1.100\n",
            "  libavfilter     6. 65.100 /  6. 65.100\n",
            "  libavresample   3.  1.  0 /  3.  1.  0\n",
            "  libswscale      4.  2.100 /  4.  2.100\n",
            "  libswresample   2.  3.100 /  2.  3.100\n",
            "  libpostproc    54.  1.100 / 54.  1.100\n"));
        assert_eq!(output.stdout_str(), concat!(
            "Hyper fast Audio and Video encoder\n",
            "usage: ffmpeg [options] [[infile options] -i infile]... {[outfile options] outfile}...\n",
            "\n",
            "Getting help:\n",
            "    -h      -- print basic options\n",
            "    -h long -- print more options\n",
            "    -h full -- print all options (including all format and codec specific options, very long)\n",
            "    -h type=name -- print all options for the named decoder/encoder/demuxer/muxer/filter\n",
            "    See man ffmpeg for detailed description of the options.\n",
            "\n",
            "Print help / information / capabilities:\n",
            "-L                  show license\n",
            "-h topic            show help\n",
            "-? topic            show help\n",
            "-help topic         show help\n",
            "--help topic        show help\n",
            "-version            show version\n",
            "-buildconf          show build configuration\n",
            "-formats            show available formats\n",
            "-devices            show available devices\n",
            "-codecs             show available codecs\n",
            "-decoders           show available decoders\n",
            "-encoders           show available encoders\n",
            "-bsfs               show available bit stream filters\n",
            "-protocols          show available protocols\n",
            "-filters            show available filters\n",
            "-pix_fmts           show available pixel formats\n",
            "-layouts            show standard channel layouts\n",
            "-sample_fmts        show available audio sample formats\n",
            "-colors             show available color names\n",
            "-sources device     list sources of the input device\n",
            "-sinks device       list sinks of the output device\n",
            "-hwaccels           show available HW acceleration methods\n",
            "\n",
            "Global options (affect whole program instead of just one file:\n",
            "-loglevel loglevel  set logging level\n",
            "-v loglevel         set logging level\n",
            "-report             generate a report\n",
            "-max_alloc bytes    set maximum size of a single allocated block\n",
            "-y                  overwrite output files\n",
            "-n                  never overwrite output files\n",
            "-ignore_unknown     Ignore unknown stream types\n",
            "-stats              print progress report during encoding\n",
            "-max_error_rate ratio of errors (0.0: no errors, 1.0: 100% error  maximum error rate\n",
            "-bits_per_raw_sample number  set the number of bits per raw sample\n",
            "-vol volume         change audio volume (256=normal)\n",
            "\n",
            "Per-file main options:\n",
            "-f fmt              force format\n",
            "-c codec            codec name\n",
            "-codec codec        codec name\n",
            "-pre preset         preset name\n",
            "-map_metadata outfile[,metadata]:infile[,metadata]  set metadata information of outfile from infile\n",
            "-t duration         record or transcode \"duration\" seconds of audio/video\n",
            "-to time_stop       record or transcode stop time\n",
            "-fs limit_size      set the limit file size in bytes\n",
            "-ss time_off        set the start time offset\n",
            "-sseof time_off     set the start time offset relative to EOF\n",
            "-seek_timestamp     enable/disable seeking by timestamp with -ss\n",
            "-timestamp time     set the recording timestamp (\'now\' to set the current time)\n",
            "-metadata string=string  add metadata\n",
            "-program title=string:st=number...  add program with specified streams\n",
            "-target type        specify target file type (\"vcd\", \"svcd\", \"dvd\", \"dv\" or \"dv50\" with optional prefixes \"pal-\", \"ntsc-\" or \"film-\")\n",
            "-apad               audio pad\n",
            "-frames number      set the number of frames to output\n",
            "-filter filter_graph  set stream filtergraph\n",
            "-filter_script filename  read stream filtergraph description from a file\n",
            "-reinit_filter      reinit filtergraph on input parameter changes\n",
            "-discard            discard\n",
            "-disposition        disposition\n",
            "\n",
            "Video options:\n",
            "-vframes number     set the number of video frames to output\n",
            "-r rate             set frame rate (Hz value, fraction or abbreviation)\n",
            "-s size             set frame size (WxH or abbreviation)\n",
            "-aspect aspect      set aspect ratio (4:3, 16:9 or 1.3333, 1.7777)\n",
            "-bits_per_raw_sample number  set the number of bits per raw sample\n",
            "-vn                 disable video\n",
            "-vcodec codec       force video codec (\'copy\' to copy stream)\n",
            "-timecode hh:mm:ss[:;.]ff  set initial TimeCode value.\n",
            "-pass n             select the pass number (1 to 3)\n",
            "-vf filter_graph    set video filters\n",
            "-ab bitrate         audio bitrate (please use -b:a)\n",
            "-b bitrate          video bitrate (please use -b:v)\n",
            "-dn                 disable data\n",
            "\n",
            "Audio options:\n",
            "-aframes number     set the number of audio frames to output\n",
            "-aq quality         set audio quality (codec-specific)\n",
            "-ar rate            set audio sampling rate (in Hz)\n",
            "-ac channels        set number of audio channels\n",
            "-an                 disable audio\n",
            "-acodec codec       force audio codec (\'copy\' to copy stream)\n",
            "-vol volume         change audio volume (256=normal)\n",
            "-af filter_graph    set audio filters\n",
            "\n",
            "Subtitle options:\n",
            "-s size             set frame size (WxH or abbreviation)\n",
            "-sn                 disable subtitle\n",
            "-scodec codec       force subtitle codec (\'copy\' to copy stream)\n",
            "-stag fourcc/tag    force subtitle tag/fourcc\n",
            "-fix_sub_duration   fix subtitles duration\n",
            "-canvas_size size   set canvas size (WxH or abbreviation)\n",
            "-spre preset        set the subtitle options to the indicated preset\n",
            "\n",
            "\n"));
    })
}
