package com.superidm.browser

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.superidm.ui.screens.BrowserScreen
import com.superidm.ui.theme.SuperIDMTheme
import dagger.hilt.android.AndroidEntryPoint

@AndroidEntryPoint
class BrowserActivity : ComponentActivity() {
    companion object {
        const val EXTRA_URL = "extra_url"
        const val EXTRA_PRIVATE_MODE = "extra_private_mode"
        
        fun start(context: Context, url: String = "https://www.google.com", privateMode: Boolean = false) {
            context.startActivity(Intent(context, BrowserActivity::class.java).apply {
                putExtra(EXTRA_URL, url)
                putExtra(EXTRA_PRIVATE_MODE, privateMode)
            })
        }
    }
    
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val initialUrl = intent.getStringExtra(EXTRA_URL) ?: "https://www.google.com"
        val isPrivate = intent.getBooleanExtra(EXTRA_PRIVATE_MODE, false)
        setContent {
            SuperIDMTheme {
                BrowserScreen(initialUrl = initialUrl, isPrivateMode = isPrivate, onNavigateBack = { finish() })
            }
        }
    }
}
