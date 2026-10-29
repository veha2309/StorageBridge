#include <flutter/runtime_effect.glsl>

// Package-owned SDF backdrop optics. Coordinates are global physical pixels;
// geometry lives in local logical pixels so scrolling never moves its bevel.
uniform vec2 uTextureSize;
uniform vec3 uLocalX;
uniform vec3 uLocalY;
uniform vec4 uForward;
uniform vec2 uSize;
uniform vec4 uRadii;
uniform float uIndex;
uniform float uBand;
uniform float uDepth;
uniform float uMagnification;
uniform float uDispersion;
uniform float uBlur;
uniform float uSaturation;
uniform vec4 uTint;
uniform float uRim;
uniform float uLight;
uniform float uDirection;
uniform float uAmbient;
uniform float uSpread;
uniform float uBlend;
uniform vec2 uFirst;
uniform vec2 uSecond;
uniform float uRadius;
uniform float uSmoothness;
uniform sampler2D uBackdrop;
out vec4 fragColor;

// Distance and its analytic gradient, evaluated together. The previous finite
// differences evaluated this entire field five times for every output pixel.
vec3 field(vec2 p) {
  if (uBlend > 0.5) {
    vec2 da=p-uFirst;
    vec2 db=p-uSecond;
    float la=max(length(da),0.0001);
    float lb=max(length(db),0.0001);
    float a=la-uRadius;
    float b=lb-uRadius;
    float k=max(uSmoothness,0.001);
    float h=clamp(0.5+0.5*(b-a)/k,0.0,1.0);
    vec2 gradient=mix(db/lb,da/la,h);
    return vec3(gradient/max(length(gradient),0.0001),mix(b,a,h)-k*h*(1.0-h));
  }
  vec2 q=p-uSize*0.5;
  float r=q.y<0.0?(q.x<0.0?uRadii.x:uRadii.y):(q.x<0.0?uRadii.w:uRadii.z);
  r=min(r,min(uSize.x,uSize.y)*0.5);
  vec2 d=abs(q)-uSize*0.5+r;
  vec2 outside=max(d,0.0);
  float len=length(outside);
  vec2 axis=d.x>d.y?vec2(1.0,0.0):vec2(0.0,1.0);
  vec2 normal=sign(q)*(len>0.0001?outside/max(len,0.0001):axis);
  return vec3(normal,len+min(max(d.x,d.y),0.0)-r);
}

vec2 globalDelta(vec2 v) {
  return vec2(dot(uForward.xy,v),dot(uForward.zw,v));
}

vec3 sampleAt(vec2 pixel) {
  vec2 uv = clamp(pixel/uTextureSize,vec2(0.00001),vec2(0.99999));
#ifdef IMPELLER_TARGET_OPENGLES
  uv.y = 1.0-uv.y;
#endif
  return texture(uBackdrop,uv).rgb;
}

vec3 blurred(vec2 p) {
  vec3 col = sampleAt(p);
  if (uBlur < 0.1) return col;
  // Five taps with the same second moment as the previous nine-tap kernel.
  // Keep blur active without nine backdrop reads per lens pixel.
  vec2 d=vec2(uBlur*0.7115);
  col *= 0.2;
  col += (sampleAt(p+d)+sampleAt(p-d)+sampleAt(p+vec2(d.x,-d.y))+
          sampleAt(p+vec2(-d.x,d.y)))*0.2;
  return col;
}

void main() {
  vec2 pixel=FlutterFragCoord().xy;
  vec3 h=vec3(pixel,1.0);
  vec2 p=vec2(dot(uLocalX,h),dot(uLocalY,h));
  vec3 geometry=field(p);
  vec2 normal=geometry.xy;
  float inside=max(-geometry.z,0.0);
  float band=max(uBand,0.001);
  float edge=1.0-smoothstep(0.0,band,inside);
  vec2 center=uSize*0.5;
  if (uBlend>0.5) center=length(p-uFirst)<length(p-uSecond)?uFirst:uSecond;
  vec2 displacement=(p-center)*(1.0/max(uMagnification,0.01)-1.0);
  displacement-=normal*sin(edge*1.5707963)*band*(uIndex-1.0)*uDepth*8.0;
  vec2 source=pixel+globalDelta(displacement);
  vec3 color=blurred(source);
  if (uDispersion>0.0) {
    vec2 separation=globalDelta(normal)*uDispersion*min(uSize.x,uSize.y)*edge;
    color.r=sampleAt(source+separation).r;
    color.b=sampleAt(source-separation).b;
  }
  float luma=dot(color,vec3(0.2126,0.7152,0.0722));
  color=mix(vec3(luma),color,uSaturation);
  color=mix(color,uTint.rgb,uTint.a);
  float highlight=pow(max(dot(normal,vec2(cos(uDirection),sin(uDirection))),0.0),
                      mix(8.0,1.0,uSpread));
  float rim=(1.0-smoothstep(0.0,max(uRim,0.001)*1.8,inside))*step(0.001,uRim);
  color+=vec3(rim*uLight*(uAmbient*0.1+highlight*0.35));
  fragColor=vec4(clamp(color,0.0,1.0),1.0);
}
