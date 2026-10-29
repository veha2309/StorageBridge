from pathlib import Path
from PIL import Image, ImageDraw
import math
root=Path(__file__).resolve().parents[1]
n=1024
im=Image.new('RGBA',(n,n)); pix=im.load()
for y in range(n):
 for x in range(n):
  t=(x+y)/(2*n); glow=max(0,1-math.hypot(x-260,y-170)/850)
  pix[x,y]=(int(17+12*glow+10*t),int(55+55*glow+15*t),int(64+57*glow+20*t),255)
mask=Image.new('L',(n,n)); d=ImageDraw.Draw(mask); d.rounded_rectangle((26,26,998,998),radius=230,fill=255); im.putalpha(mask)
d=ImageDraw.Draw(im)
d.rounded_rectangle((29,29,995,995),radius=228,outline=(175,242,229,100),width=5)
# The cloud flows into a bridge deck: one continuous, readable silhouette.
d.rounded_rectangle((214,340,810,602),radius=120,fill=(208,248,240,255))
d.ellipse((305,221,616,536),fill=(208,248,240,255))
d.ellipse((499,289,743,533),fill=(208,248,240,255))
# Negative space arch and two supports form the bridge.
d.rounded_rectangle((294,465,730,754),radius=130,fill=(26,78,86,255))
d.rectangle((294,598,730,785),fill=(26,78,86,255))
d.rounded_rectangle((218,500,305,777),radius=35,fill=(208,248,240,255))
d.rounded_rectangle((719,500,806,777),radius=35,fill=(208,248,240,255))
d.rounded_rectangle((165,593,860,650),radius=28,fill=(112,223,200,255))
d.rounded_rectangle((192,594,831,605),radius=5,fill=(231,255,248,255))
im.resize((512,512),Image.Resampling.LANCZOS).save(root/'assets/storage_bridge.png')
im.save(root/'windows/runner/resources/app_icon.ico',sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
