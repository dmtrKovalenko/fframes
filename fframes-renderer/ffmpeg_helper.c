#include <stdio.h>
#include <libavutil/opt.h>
#include <libavutil/timestamp.h>
#include "libavcodec/avcodec.h"
#include "libavformat/avformat.h"

const char *av_error_to_string(int error_code)
{
  return av_err2str(error_code);
}

void make_stereo_layout_channel(AVCodecContext *c, AVCodec *codec)
{
  av_channel_layout_copy(&c->ch_layout, &(AVChannelLayout)AV_CHANNEL_LAYOUT_MONO);
}

void log_packet(AVStream *stream, AVPacket *pkt)
{
  printf("pts:%s pts_time:%s dts:%s dts_time:%s duration:%s duration_time:%s stream_index:%d\n",
         av_ts2str(pkt->pts), av_ts2timestr(pkt->pts, &stream->time_base),
         av_ts2str(pkt->dts), av_ts2timestr(pkt->dts, &stream->time_base),
         av_ts2str(pkt->duration), av_ts2timestr(pkt->duration, &stream->time_base),
         pkt->stream_index);
}