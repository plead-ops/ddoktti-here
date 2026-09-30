"""Reviewed jacket hem repairs only; character poses come from raster artwork.
Landing, hurt and sleepy frames are generated PNGs traced by trace-pet-svg.py.
"""
import xml.etree.ElementTree as ET
HEMS={'emotions-v2-0': 'M163 225 Q177 231 194 226 L201 221 M204 228 Q220 232 235 225', 'emotions-v2-1': 'M167 223 Q181 229 197 223 L202 220 M205 225 Q223 229 238 222', 'emotions-v2-2': 'M164 227 Q180 231 194 226 L201 222 M204 228 Q218 232 232 226', 'emotions-v2-3': 'M166 226 Q181 231 196 226 L201 222 M204 229 Q219 232 233 226', 'emotions-v2-4': 'M165 225 Q180 231 195 226 L200 222 M203 228 Q217 231 230 225', 'emotions-v2-5': 'M169 226 Q180 231 194 226 L200 222 M203 229 Q216 232 230 227', 'emotions-v2-6': 'M158 224 Q170 231 185 228 L192 224 M196 231 Q211 234 226 229', 'emotions-v2-7': 'M174 228 Q184 231 196 226 L201 222 M204 229 Q218 232 231 226', 'emotions-v2-8': 'M166 226 Q180 231 195 226 L201 222 M204 229 Q219 232 233 226', 'emotions-v2-9': 'M169 227 Q180 231 195 226 L201 223 M204 229 Q218 231 231 226', 'emotions-v2-10': 'M179 229 Q187 231 196 226 L201 222 M204 229 Q219 232 232 226', 'emotions-v2-11': 'M168 226 Q181 231 196 226 L201 222 M204 229 Q219 232 233 226', 'emotions-v2-12': 'M163 225 Q177 231 194 226 L201 222 M204 229 Q220 232 234 225', 'emotions-v2-13': 'M166 224 Q181 230 197 224 L203 220 M206 227 Q223 230 238 224', 'emotions-v2-14': 'M166 225 Q182 232 197 227 L203 223 M206 230 Q223 233 239 227', 'emotions-v2-15': 'M165 224 Q181 230 197 224 L203 220 M206 227 Q225 230 241 224', 'emotions-v2-16': 'M167 226 Q181 231 195 226 L201 222 M204 229 Q218 232 232 226', 'emotions-v2-17': 'M169 228 Q182 232 196 226 L201 222 M204 229 Q219 232 232 226', 'emotions-v2-18': 'M170 226 Q181 231 196 226 L201 221 M204 229 Q217 230 227 226', 'emotions-v2-19': 'M167 226 Q181 231 196 226 L201 222 M204 229 Q219 232 234 226'}

def repair(key,drawing):
    line=HEMS.get(key)
    if not line:return drawing
    # Confine each seam to existing cloth. It must never extend beyond a tilted
    # jacket or paint over a hand/accessory, even when rendered at 5x size.
    paths=ET.fromstring('<svg>'+drawing+'</svg>')
    cloth=''.join(ET.tostring(p,encoding='unicode') for p in paths if p.get('fill')=='#26374F')
    clip=f'hem-{key}'
    return drawing+f'<defs><clipPath id="{clip}">{cloth}</clipPath></defs><path clip-path="url(#{clip})" d="{line}" fill="none" stroke="#081724" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"/>'
