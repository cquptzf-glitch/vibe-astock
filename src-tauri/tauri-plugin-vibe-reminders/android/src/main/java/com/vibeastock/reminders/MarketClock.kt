package com.vibeastock.reminders

import java.text.ParsePosition
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale
import java.util.TimeZone

internal data class MarketClock(val millis:Long,val dayKey:String,val date:String,val hour:Int,val minute:Int,val weekend:Boolean,val clock:String)
/** java.util APIs work on the supported Android 24/25 devices without desugaring. */
internal fun marketClock(millis:Long=System.currentTimeMillis()):MarketClock {
    val zone=TimeZone.getTimeZone("Asia/Shanghai")
    val calendar=Calendar.getInstance(zone).apply { timeInMillis=millis }
    fun format(pattern:String)=SimpleDateFormat(pattern,Locale.ROOT).apply { timeZone=zone }.format(Date(millis))
    return MarketClock(millis,format("yyyyMMdd"),format("yyyy-MM-dd"),calendar.get(Calendar.HOUR_OF_DAY),calendar.get(Calendar.MINUTE),calendar.get(Calendar.DAY_OF_WEEK) in listOf(Calendar.SATURDAY,Calendar.SUNDAY),format("HH:mm:ss"))
}
internal fun quoteMillis(stamp:String):Long? {
    if(!Regex("^[0-9]{14}$").matches(stamp))return null
    val format=SimpleDateFormat("yyyyMMddHHmmss",Locale.ROOT).apply { isLenient=false;timeZone=TimeZone.getTimeZone("Asia/Shanghai") }
    val position=ParsePosition(0)
    val date=format.parse(stamp,position)?:return null
    return if(position.index==stamp.length)date.time else null
}
