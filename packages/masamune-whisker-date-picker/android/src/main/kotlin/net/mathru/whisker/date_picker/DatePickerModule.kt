// Native OS date and time picker dialogs.

package net.mathru.whisker.date_picker

import android.app.Activity
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.text.format.DateFormat
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class DatePickerModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("DatePicker")

        // pickDate({ year, month(1-12), day, minMillis?, maxMillis? }) -> { year, month, day } | null (cancelled)
        AsyncFunction("pickDate") { args, promise ->
            val activity: Activity? = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val json = Bridge.args(args)
            Bridge.onMain {
                var settled = false
                val dialog = DatePickerDialog(
                    activity,
                    android.R.style.Theme_DeviceDefault_Dialog_Alert,
                    { _, year, month, day ->
                        settled = true
                        Bridge.resolve(promise, mapOf("year" to year, "month" to month + 1, "day" to day))
                    },
                    json.optInt("year", 2001),
                    json.optInt("month", 1) - 1,
                    json.optInt("day", 1),
                )
                if (json.has("minMillis")) dialog.datePicker.minDate = json.getLong("minMillis")
                if (json.has("maxMillis")) dialog.datePicker.maxDate = json.getLong("maxMillis")
                dialog.setOnDismissListener {
                    if (!settled) Bridge.resolve(promise, null)
                }
                dialog.show()
            }
        }

        // pickTime({ hour, minute }) -> { hour, minute } | null
        AsyncFunction("pickTime") { args, promise ->
            val activity: Activity? = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val json = Bridge.args(args)
            Bridge.onMain {
                var settled = false
                val dialog = TimePickerDialog(
                    activity,
                    android.R.style.Theme_DeviceDefault_Dialog_Alert,
                    { _, hour, minute ->
                        settled = true
                        Bridge.resolve(promise, mapOf("hour" to hour, "minute" to minute))
                    },
                    json.optInt("hour", 8),
                    json.optInt("minute", 0),
                    DateFormat.is24HourFormat(activity),
                )
                dialog.setOnDismissListener {
                    if (!settled) Bridge.resolve(promise, null)
                }
                dialog.show()
            }
        }
    }
}
