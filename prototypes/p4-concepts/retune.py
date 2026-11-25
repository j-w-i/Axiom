import re
k=open('lib/kit.dart').read(); d=open('lib/concept_d.dart').read()
def sub(s,pairs):
    for a,b in pairs:
        assert a in s,a
        s=s.replace(a,b)
    return s
# --- kit: accent is cobalt, secondary is a greyed steel; nothing tinted sky
k=sub(k,[
 ("steel = Color(0xFF7F9DBD), steelDim = Color(0xFF3C526B)","steel = Color(0xFF8490A6), steelDim = Color(0xFF3B4352)"),
 ("volt = Color(0xFF4E9DFF), voltDeep = Color(0xFF1F63C4)","volt = Color(0xFF3F68E0), voltDeep = Color(0xFF2A49A8)"),
 ("amber = Color(0xFFE2AC55), red = Color(0xFFE56B62), green = Color(0xFF63B68F)","amber = Color(0xFFD2A052), red = Color(0xFFD9655E), green = Color(0xFF5FA383)"),
 ("ink1 = Color(0xFFE4EAF2), ink2 = Color(0xFFAAB6C5), ink3 = Color(0xFF7C899B), ink4 = Color(0xFF536072)","ink1 = Color(0xFFE6E9EF), ink2 = Color(0xFFB0B7C3), ink3 = Color(0xFF868F9E), ink4 = Color(0xFF626B7A)"),
 ("keyVolt = Mat(top: Color(0xFF3F8BEB), bottom: Color(0xFF2266C6), r: 4, lit: .32, edge: Color(0xFF0B2C5C)","keyVolt = Mat(top: Color(0xFF3C60D0), bottom: Color(0xFF3153BE), r: 4, lit: .14, edge: Color(0xFF1A2E6E)"),
 ("keyAmber = Mat(top: Color(0xFF8A6A35), bottom: Color(0xFF6B5026), r: 4, lit: .25, edge: Color(0xFF2B1F0C)","keyAmber = Mat(top: Color(0xFF6E5732), bottom: Color(0xFF64502F), r: 4, lit: .12, edge: Color(0xFF2E2412)"),
 # inset: shorter, softer
 ("math.min(9, s.height)), Paint()..shader = ui.Gradient.linear(Offset.zero, const Offset(0, 9), [a(const Color(0xFF000000), m.inset * .75)","math.min(5, s.height)), Paint()..shader = ui.Gradient.linear(Offset.zero, const Offset(0, 5), [a(const Color(0xFF000000), m.inset * .75)"),
 ("[(0.0, 5.0), (s.width, s.width - 5)]","<(double, double)>[]"),
 ("copyWith(shadows: const [Shadow(color: Color(0xAA000000), offset: Offset(0, -.6))])","copyWith()"),
 # lamp: a flat capsule, no gloss
 ("v.drawRRect(r.shift(const Offset(0, .8)), Paint()..color = const Color(0x1FCFE0F5)); v.drawRRect(r, Paint()..color = const Color(0xFF05080C));",""),
 ("v.drawRRect(i, Paint()..color = c); v.drawRRect(RRect.fromRectAndRadius(Rect.fromLTWH(i.left + .5, i.top + .4, i.width - 1, i.height * .42), const Radius.circular(2)), Paint()..color = a(const Color(0xFFFFFFFF), .38));","v.drawRRect(i, Paint()..color = c);"),
 ("Color.lerp(const Color(0xFF0B1017), c, .22)!","const Color(0xFF454C58)"),
 ("Color.lerp(const Color(0xFF3B5C82), hot, i / n)!","Color.lerp(const Color(0xFF56607A), hot, i / n)!"),
 ("const Color(0xFF82AEE0) : k[2]","const Color(0xFF9AA5C9) : k[2]"),("const Color(0xFF9CC3A5)","const Color(0xFFA3B59C)"),("const Color(0xFFC9D8EA)","const Color(0xFFD5DAE3)"),("const Color(0xFFB7A9DC)","const Color(0xFFB3A6C4)"),("const Color(0xFF9DABBC)","const Color(0xFFA5ADBA)"),
 ("[const Color(0xFFB9D6F7), const Color(0xFF5B7FA8)]","[const Color(0xFFD9DDE6), const Color(0xFF8B94A6)]"),("[const Color(0xFF3E86E6), const Color(0xFF1C4378)]","[const Color(0xFF4A6CD4), const Color(0xFF263E8C)]"),("..color = const Color(0xFF4E9DFF)); }","..color = const Color(0xFF4A6CD4)); }"),
 ("color: a(const Color(0xFFCFE0F5), m.lip)" if False else "a(const Color(0xFFCFE0F5), m.lip)","a(const Color(0xFFFFFFFF), m.lip)"),
])
d=d.replace("import 'kit.dart';","import 'kit_e.dart';").replace('ConceptD','ConceptE')
d=sub(d,[
 ("gChassisT = Color(0xFF24272C), gChassisB = Color(0xFF17191D)","gChassisT = Color(0xFF1E2025), gChassisB = Color(0xFF191B1F)"),
 ("gPlate = Mat(top: Color(0xFF32363D), bottom: Color(0xFF262A30), r: 5, lit: .13, edge: Color(0xFF0B0C0F), drop: [BoxShadow(color: Color(0x80000000), blurRadius: 6, offset: Offset(0, 2.5))])","gPlate = Mat(top: Color(0xFF2A2D33), bottom: Color(0xFF26292E), r: 5, lit: .065, edge: Color(0xFF131418), drop: [BoxShadow(color: Color(0x40000000), blurRadius: 3, offset: Offset(0, 1))])"),
 ("gPlateSel = Mat(top: Color(0xFF3B4858), bottom: Color(0xFF2B3542), r: 5, lit: .22, edge: Color(0xFF0A0C10), drop: [BoxShadow(color: Color(0x99000000), blurRadius: 12, offset: Offset(0, 5))])","gPlateSel = Mat(top: Color(0xFF353943), bottom: Color(0xFF2F333C), r: 5, lit: .11, edge: Color(0xFF131418), drop: [BoxShadow(color: Color(0x59000000), blurRadius: 7, offset: Offset(0, 2))])"),
 ("gBay = Mat(top: Color(0xFF080D15), bottom: Color(0xFF0B121D), r: 5, inset: .8, lip: .11, edge: Color(0xFF030508))","gBay = Mat(top: Color(0xFF0E121A), bottom: Color(0xFF10151F), r: 5, inset: .34, lip: .045, edge: Color(0xFF0B0D12))"),
 ("gKey = Mat(top: Color(0xFF474D57), bottom: Color(0xFF343941), r: 4, lit: .19, edge: Color(0xFF0D0F12), drop: [BoxShadow(color: Color(0x73000000), blurRadius: 2, offset: Offset(0, 1.5))])","gKey = Mat(top: Color(0xFF3A3E46), bottom: Color(0xFF353941), r: 4, lit: .075, edge: Color(0xFF17191D), drop: [BoxShadow(color: Color(0x33000000), blurRadius: 1.5, offset: Offset(0, 1))])"),
 ("gDown = Mat(top: Color(0xFF14171B), bottom: Color(0xFF1B1F25), r: 4, inset: .8, lip: .09, edge: Color(0xFF050608))","gDown = Mat(top: Color(0xFF15171B), bottom: Color(0xFF181B20), r: 4, inset: .3, lip: .04, edge: Color(0xFF101115))"),
 ("gWell = Mat(top: Color(0xFF111316), bottom: Color(0xFF16181C), r: 7, inset: .9, lip: .10, edge: Color(0xFF060709))","gWell = Mat(top: Color(0xFF141619), bottom: Color(0xFF16181C), r: 7, inset: .34, lip: .045, edge: Color(0xFF0E0F12))"),
 ("const [Color(0x1AB9CCE6), Color(0x00B9CCE6)]","const [Color(0x0EFFFFFF), Color(0x00FFFFFF)]"),("Color(0x24E4EEFF)","Color(0x12FFFFFF)"),
 ("Color(0xFF0B0C0E)","Color(0xFF131518)"),("Color(0x14FFFFFF)","Color(0x0DFFFFFF)"),
 (".copyWith(shadows: const [Shadow(color: Color(0xCC000000), offset: Offset(0, -.6))])",""),
 ("dInk2 = Color(0xFFB4BBC5), dInk3 = Color(0xFF878F9B), dInk4 = Color(0xFF5E6672)","dInk2 = Color(0xFFBCC1CA), dInk3 = Color(0xFF959BA6), dInk4 = Color(0xFF7C838F)"),
 ("Color.lerp(const Color(0xFF34557D), const Color(0xFF5FA6FA), i / 16)!","Color.lerp(const Color(0xFF4C5670), const Color(0xFF4A6CD4), i / 16)!"),("Color.lerp(const Color(0xFF34557D), const Color(0xFF5FA6FA), i / n)!","Color.lerp(const Color(0xFF4C5670), const Color(0xFF4A6CD4), i / n)!"),
 ("Color(0xFF141C28)","Color(0xFF1C212B)"),
 ("ms(15, 500, const Color(0xFFA6CBF7), 1.2)","ms(15, 500, ink1, 1.2)"),
 ("c: const Color(0xFF9FC0E6)","c: const Color(0xFFA7AFBD)"),("Color(0xFFD6DEE8)","Color(0xFFDCE0E7)"),
 ("Color(0xFF5D7CA0)","Color(0xFF5A6478)"),("Color(0xFF1B2636)","Color(0xFF222936)"),("Color(0xFF04070B))..color" if False else "Color(0xFF141D2A)","Color(0xFF1B202A)"),
 ("Color(0xFF0E1622)","Color(0xFF141922)"),("Color(0xFF182334)","Color(0xFF212734)"),
 ("Mat(top: Color(0xFF3A352B), bottom: Color(0xFF2C2A26), r: 5, lit: .16, edge: Color(0xFF14110B), drop: [BoxShadow(color: Color(0x80000000), blurRadius: 6, offset: Offset(0, 2))])","Mat(top: Color(0xFF2E2B26), bottom: Color(0xFF2B2924), r: 5, lit: .07, edge: Color(0xFF191713), drop: [BoxShadow(color: Color(0x40000000), blurRadius: 3, offset: Offset(0, 1))])"),
 ("Color(0xFFF3CF8E)","Color(0xFFDDBB83)"),("Color(0xFFF1E2C6)","Color(0xFFE9DFCD)"),("Color(0xFFFFF0D2)","Color(0xFFF2E6CE)"),
 ("const Color(0xFF2D62A0)","const Color(0xFF2F437F)"),("'ask' => const Color(0xFF8D6B30)","'ask' => const Color(0xFF6E5732)"),("const Color(0xFF7C3F3C)","const Color(0xFF6B3A39)"),("const Color(0xFF3B4456)","const Color(0xFF373C48)"),("const Color(0xFF2A3D56)","const Color(0xFF2B3140)"),
 ("Color(0xFF101C2D)","Color(0xFF171C27)"),("Color(0x08A9C8F0)","Color(0x06FFFFFF)"),("Color(0xFF55779E)","Color(0xFF5C6780)"),("Color(0x8CCFE4FF)","Color(0x80D5DAE6)"),("Color(0xFFE3EDF8)","Color(0xFFE2E5EC)"),
 ("lit: live ? .34 : .16","lit: live ? .16 : .07"),(".07)!, bottom: Color.lerp(col, const Color(0xFF000000), .22)!",".03)!, bottom: Color.lerp(col, const Color(0xFF000000), .08)!"),
 ("Color(0xFF131C29)","Color(0xFF1A1F29)"),("const Color(0xFF17212F) : const Color(0xFF111926)","const Color(0xFF1D222C) : const Color(0xFF161B24)"),
 # the add key becomes a normal key at the foot
 ("Expanded(child: dbtn('plus', h: 200)), gap(5), dbtn('list', h: 28, m: gDown, c: volt)","const Spacer(), dbtn('plus', h: 28), gap(5), dbtn('list', h: 28)"),
 ("Expanded(child: dbtn('plus', h: 200)), gap(5), dbtn('list', h: 28)","const Spacer(), dbtn('plus', h: 28), gap(5), dbtn('list', h: 28, m: gDown)"),
])
# play icon engaged: ink on a pressed key, cobalt reserved for lamps/primary
d=d.replace("dbtn('play', m: gDown, c: volt, fill: true","dbtn('play', m: gDown, c: ink1, fill: true")
open('lib/kit_e.dart','w').write(k); open('lib/concept_e.dart','w').write(d)
