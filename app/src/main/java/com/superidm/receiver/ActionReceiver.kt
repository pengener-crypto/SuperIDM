package com.superidm.receiver

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.superidm.service.DownloadService
import dagger.hilt.android.AndroidEntryPoint

@AndroidEntryPoint
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return
        val downloadId = intent.getStringExtra(DownloadService.EXTRA_DOWNLOAD_ID)

        val serviceIntent = Intent(context, DownloadService::class.java).apply {
            this.action = action
            if (downloadId != null) {
                putExtra(DownloadService.EXTRA_DOWNLOAD_ID, downloadId)
            }
        }
        
        context.startService(serviceIntent)
    }
}
